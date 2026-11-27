// The author's items in the library service (`docs/specs/interface-v1.md`,
// Shares), as the website's server reads them: the published items of the
// library account, their versions and what withdrew them, and the author's
// withdrawal (story S17).

import { LIBRARY_API } from '$app/env/private';
import { Refused } from './accounts.ts';
import { Unavailable } from './catalogue.ts';

/** What withdrew an item or a version: its author, a switch-off or moderation. */
export type Withdrawal = {
	kind: 'author' | 'switch_off' | 'moderation';
	reason: string;
	withdrawn_at: string;
};

/** A published item of the author's, its versions newest first. */
export type AuthoredItem = {
	item: string;
	title: string;
	/** What withdrew every version, if anything did. */
	withdrawal: Withdrawal | null;
	versions: { version: number; published_at: string; withdrawal: Withdrawal | null }[];
};

export function authored(fetch: typeof globalThis.fetch, root: string = LIBRARY_API) {
	async function call<T>(method: string, path: string, session: string, body?: unknown) {
		const headers: Record<string, string> = { authorization: `Bearer ${session}` };
		if (body !== undefined) headers['content-type'] = 'application/json';
		let response: Response;
		try {
			response = await fetch(new URL(path, root), {
				method,
				headers,
				body: body === undefined ? undefined : JSON.stringify(body)
			});
		} catch (cause) {
			throw new Unavailable('the library API is unreachable', { cause });
		}
		if (response.status >= 500)
			throw new Unavailable(`the library API answered ${response.status}`);
		const answer = (await response.json()) as T & { error?: string };
		if (!response.ok) throw new Refused(response.status, answer.error ?? 'bad_request');
		return answer;
	}

	return {
		async items(session: string) {
			return (await call<{ items: AuthoredItem[] }>('GET', '/api/account/items', session)).items;
		},
		/** Withdraws the item, or only `version` of it, for good. */
		withdraw(session: string, item: string, version: number | null) {
			return call<{ item: string; version: number | null } & Withdrawal>(
				'POST',
				`/api/account/items/${encodeURIComponent(item)}/withdrawal`,
				session,
				{ version }
			);
		}
	};
}
