// The sharing part of the library service (`docs/specs/interface-v1.md`,
// Shares), as the website's server reads it: the sharing terms, and the
// author's shares with their outcome and the day's quota. Sharing itself
// happens in the parent app.

import { LIBRARY_API } from '$app/env/private';
import { Refused } from './accounts.ts';
import { Unavailable } from './catalogue.ts';

/** One reason the review refused a share for, per question. */
export type RefusalReason = {
	/** The question's place in the item, from 1; null for the whole item. */
	question: number | null;
	reason: string;
	explanation: string;
};

/** A share, as its author sees it. */
export type Share = {
	share: string;
	item: string | null;
	title: string;
	outcome: 'published' | 'held' | 'refused';
	version: number | null;
	reasons: RefusalReason[];
	shared_at: string;
};

/** Where the account stands for sharing. */
export type Standing = {
	sharing_terms: { current: string; accepted: string | null };
	quota: { shares_per_day: number; shared_today: number; next_share_at: string | null };
	shares: Share[];
};

export function shares(fetch: typeof globalThis.fetch, root: string = LIBRARY_API) {
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
			throw new Unavailable('the sharing API is unreachable', { cause });
		}
		if (response.status >= 500)
			throw new Unavailable(`the sharing API answered ${response.status}`);
		const answer = (await response.json()) as T & { error?: string };
		if (!response.ok) throw new Refused(response.status, answer.error ?? 'bad_request');
		return answer;
	}

	return {
		standing(session: string) {
			return call<Standing>('GET', '/api/account/shares', session);
		},
		/** Accepts the sharing terms of `version`, the ones the page showed. */
		accept(session: string, version: string) {
			return call<Standing['sharing_terms']>('PUT', '/api/account/sharing-terms', session, {
				version
			});
		}
	};
}
