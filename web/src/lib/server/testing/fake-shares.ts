// A stand-in for the sharing routes of the library service, over the fake
// accounts: a test sets the shares each account made.

import type { FakeAccounts } from './fake-accounts.ts';
import type { AuthorNotification, Share } from '../shares.ts';

export const TERMS = '2026-11';
export const SHARES_PER_DAY = 10;

export type FakeShares = {
	/** The sharing routes' answers; null for another path. */
	fetch: (input: Request) => Promise<Response> | null;
	/** The terms accepted, by Minute Jar account. */
	accepted: Map<string, string>;
	/** The shares, newest first, by Minute Jar account. */
	shares: Map<string, Share[]>;
	/** How many shares each Minute Jar account made today. */
	today: Map<string, number>;
	/** What each Minute Jar account's items' author must be told. */
	notifications: Map<string, AuthorNotification[]>;
	/** The appeals received, by report, with their statement. */
	appeals: Map<string, string>;
};

const json = (body: unknown, status = 200) => Response.json(body, { status });
const refused = (status: number, error: string) => json({ error }, status);

export function fakeShares(accounts: FakeAccounts): FakeShares {
	async function answer(request: Request): Promise<Response> {
		const { pathname } = new URL(request.url);
		const bearer = request.headers.get('authorization')?.replace(/^Bearer /, '') ?? '';
		const subject = accounts.sessions.get(bearer);
		if (!subject) return refused(401, 'signed_out');
		if (!accounts.accounts.get(subject)) return refused(404, 'no_account');

		if (pathname === '/api/account/sharing-terms' && request.method === 'PUT') {
			const { version } = (await request.json()) as { version: string };
			if (version !== TERMS) return refused(409, 'outdated_sharing_terms');
			fake.accepted.set(subject, version);
			return json({ current: TERMS, accepted: TERMS });
		}
		if (pathname === '/api/account/notifications' && request.method === 'GET') {
			return json({ notifications: fake.notifications.get(subject) ?? [] });
		}
		const appealed = pathname.match(/^\/api\/account\/reports\/([^/]+)\/appeal$/);
		if (appealed && request.method === 'POST') {
			const report = appealed[1];
			const theirs = (fake.notifications.get(subject) ?? []).some(
				(told) => told.report === report && told.decision
			);
			if (!theirs) return refused(404, 'unknown_report');
			if (fake.appeals.has(report)) return refused(409, 'already_appealed');
			const { statement } = (await request.json()) as { statement: string };
			if (!statement.trim()) return refused(400, 'statement_required');
			fake.appeals.set(report, statement);
			return json({ appeal: `appeal-of-${report}` }, 202);
		}
		if (pathname === '/api/account/shares' && request.method === 'GET') {
			const today = fake.today.get(subject) ?? 0;
			return json({
				sharing_terms: { current: TERMS, accepted: fake.accepted.get(subject) ?? null },
				quota: {
					shares_per_day: SHARES_PER_DAY,
					shared_today: today,
					next_share_at: today >= SHARES_PER_DAY ? '2026-11-28T00:00:00Z' : null
				},
				shares: fake.shares.get(subject) ?? []
			});
		}
		return refused(404, 'not_found');
	}

	const fake: FakeShares = {
		accepted: new Map(),
		shares: new Map(),
		today: new Map(),
		notifications: new Map(),
		appeals: new Map(),
		fetch(request) {
			const { pathname } = new URL(request.url);
			const ours =
				pathname === '/api/account/shares' ||
				pathname === '/api/account/sharing-terms' ||
				pathname === '/api/account/notifications' ||
				pathname.startsWith('/api/account/reports/');
			return ours ? answer(request) : null;
		}
	};
	return fake;
}
