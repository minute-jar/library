// The sharing part of the library service (`docs/specs/interface-v1.md`,
// Shares), as the website's server reads it: the sharing terms, and the
// author's shares with their outcome and the day's quota. Sharing itself
// happens in the parent app.

import { LIBRARY_API } from '$app/env/private';
import { caller } from './api.ts';

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

/** What the author must be told about a report on one of their items. */
export type AuthorNotification = {
	item: string;
	title: string;
	report: string;
	reason: string;
	details: string | null;
	/** Null for a report to answer by a new version; the decision otherwise. */
	decision: { outcome: 'upheld' | 'rejected'; reasons: string } | null;
	/** An appeal's decision, from the author or the reporter. */
	appeal: { outcome: 'maintained' | 'reversed'; reasons: string } | null;
	at: string;
};

export function shares(fetch: typeof globalThis.fetch, root: string = LIBRARY_API) {
	const call = caller(fetch, root, 'sharing');

	return {
		standing(session: string) {
			return call<Standing>('GET', '/api/account/shares', { session });
		},
		async notifications(session: string) {
			return (
				await call<{ notifications: AuthorNotification[] }>('GET', '/api/account/notifications', {
					session
				})
			).notifications;
		},
		/** The author's appeal of the decision on a report of their item. */
		appeal(session: string, report: string, statement: string) {
			return call<{ appeal: string }>(
				'POST',
				`/api/account/reports/${encodeURIComponent(report)}/appeal`,
				{ session, body: { statement } }
			);
		},
		/** Accepts the sharing terms of `version`, the ones the page showed. */
		accept(session: string, version: string) {
			return call<Standing['sharing_terms']>('PUT', '/api/account/sharing-terms', {
				session,
				body: {
					version
				}
			});
		}
	};
}
