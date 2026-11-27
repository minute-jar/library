// A stand-in for the author's items routes of the library service, over the
// fake accounts: a test sets the items each account published.

import type { FakeAccounts } from './fake-accounts.ts';
import type { AuthoredItem, Withdrawal } from '../authored.ts';

export type FakeAuthored = {
	/** The author's items routes' answers; null for another path. */
	fetch: (input: Request) => Promise<Response> | null;
	/** The published items, by Minute Jar account. */
	items: Map<string, AuthoredItem[]>;
	/** The items every account without its own has published, copied for each. */
	everyone: AuthoredItem[];
	/** The withdrawals made, in order. */
	withdrawn: { item: string; version: number | null }[];
};

const json = (body: unknown, status = 200) => Response.json(body, { status });
const refused = (status: number, error: string) => json({ error }, status);

export function fakeAuthored(accounts: FakeAccounts): FakeAuthored {
	async function answer(request: Request): Promise<Response> {
		const { pathname } = new URL(request.url);
		const bearer = request.headers.get('authorization')?.replace(/^Bearer /, '') ?? '';
		const subject = accounts.sessions.get(bearer);
		if (!subject) return refused(401, 'signed_out');
		if (!accounts.accounts.get(subject)) return refused(404, 'no_account');
		if (!fake.items.has(subject)) fake.items.set(subject, structuredClone(fake.everyone));
		const items = fake.items.get(subject) ?? [];

		if (pathname === '/api/account/items' && request.method === 'GET') {
			return json({ items });
		}
		const asked = pathname.match(/^\/api\/account\/items\/([^/]+)\/withdrawal$/);
		if (asked && request.method === 'POST') {
			const { version } = (await request.json()) as { version: number | null };
			const state = await accounts.fetch(
				new Request('http://fake/api/session', { headers: { authorization: `Bearer ${bearer}` } })
			);
			const { strong_sign_in_until } = (await state?.json()) as {
				strong_sign_in_until: string | null;
			};
			if (!strong_sign_in_until) return refused(403, 'strong_sign_in_required');
			const item = items.find((theirs) => theirs.item === asked[1]);
			if (!item) return refused(404, 'unknown_item');
			const target =
				version === null ? item : item.versions.find((each) => each.version === version);
			if (!target) return refused(404, 'unknown_version');
			if (item.withdrawal?.kind === 'author' || target.withdrawal?.kind === 'author')
				return refused(409, 'already_withdrawn');
			const withdrawal: Withdrawal = {
				kind: 'author',
				reason: 'author_choice',
				withdrawn_at: '2026-11-27T10:10:00Z'
			};
			target.withdrawal = withdrawal;
			fake.withdrawn.push({ item: item.item, version });
			return json({ item: item.item, version, ...withdrawal });
		}
		return refused(404, 'not_found');
	}

	const fake: FakeAuthored = {
		items: new Map(),
		everyone: [],
		withdrawn: [],
		fetch(request) {
			const { pathname } = new URL(request.url);
			return pathname.startsWith('/api/account/items') ? answer(request) : null;
		}
	};
	return fake;
}
