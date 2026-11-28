// Comments from library accounts (`docs/specs/catalogue-api.md`, Comments),
// as the website's server sends them: a comment on an item under the
// account's pseudonym, reviewed before it shows, and the anonymous report
// of a published comment. A comment is addressed to an item, never to an
// account: there are no private messages.

import { LIBRARY_API } from '$app/env/private';
import { caller } from './api.ts';
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
	const call = caller(fetch, root, 'comments');

	return {
		/** Comments `text` on `item` from the session's library account; answers the review's outcome. */
		comment(session: string, item: string, text: string) {
			return call<OwnComment>('POST', `/api/items/${encodeURIComponent(item)}/comments`, {
				session,
				body: { text }
			});
		},
		/** Reports a published comment, anonymously; answers the report's identifier. */
		report(comment: string, reason: string, details: string) {
			return call<{ report: string }>(
				'POST',
				`/api/comments/${encodeURIComponent(comment)}/reports`,
				{ body: details.trim() ? { reason, details } : { reason } }
			);
		}
	};
}
