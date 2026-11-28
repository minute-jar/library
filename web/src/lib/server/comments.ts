// Comments from library accounts (`docs/specs/catalogue-api.md`, Comments),
// as the website's server sends them: a comment on an item under the
// account's pseudonym, reviewed before it shows, and the anonymous report
// of a published comment. A comment is addressed to an item, never to an
// account: there are no private messages.

import { LIBRARY_API } from '$app/env/private';
import { Refused } from './accounts.ts';
import { Unavailable } from './catalogue.ts';
import type { RefusalReason } from './shares.ts';

export { REPORT_REASONS } from '../report-reasons.ts';

/** A comment, as its author sees it once reviewed. */
export type OwnComment = {
	comment: string;
	item: string;
	text: string;
	outcome: 'published' | 'held' | 'refused' | 'removed';
	reasons: RefusalReason[];
	commented_at: string;
};

export function comments(fetch: typeof globalThis.fetch, root: string = LIBRARY_API) {
	async function call<T>(method: string, path: string, body: unknown, session?: string) {
		const headers: Record<string, string> = { 'content-type': 'application/json' };
		if (session) headers.authorization = `Bearer ${session}`;
		let response: Response;
		try {
			response = await fetch(new URL(path, root), {
				method,
				headers,
				body: JSON.stringify(body)
			});
		} catch (cause) {
			throw new Unavailable('the comments API is unreachable', { cause });
		}
		if (response.status >= 500)
			throw new Unavailable(`the comments API answered ${response.status}`);
		const answer = (await response.json()) as T & { error?: string };
		if (!response.ok) throw new Refused(response.status, answer.error ?? 'bad_request');
		return answer;
	}

	return {
		/** Comments `text` on `item` from the session's library account; answers the review's outcome. */
		comment(session: string, item: string, text: string) {
			return call<OwnComment>(
				'POST',
				`/api/items/${encodeURIComponent(item)}/comments`,
				{ text },
				session
			);
		},
		/** Reports a published comment, anonymously; answers the report's identifier. */
		report(comment: string, reason: string, details: string) {
			return call<{ report: string }>(
				'POST',
				`/api/comments/${encodeURIComponent(comment)}/reports`,
				details.trim() ? { reason, details } : { reason }
			);
		}
	};
}
