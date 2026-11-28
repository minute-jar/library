// A stand-in for the author profiles routes of the library service, over
// the fake accounts: a test sets each account's own profile; anyone sees it
// without the suggestions received, under the account's pseudonym.

import type { FakeAccounts } from './fake-accounts.ts';
import type { Entry } from '../catalogue.ts';
import type { OwnProfile, Profile } from '../profiles.ts';

export type FakeProfiles = {
	/** The profiles routes' answers; null for another path. */
	fetch: (input: Request) => Promise<Response> | null;
	/** Each account's own profile, by Minute Jar account. */
	own: Map<string, OwnProfile>;
	/** The items of every account without a profile of its own, for the smoke tests. */
	everyone: OwnProfile['items'];
};

const json = (body: unknown, status = 200, cache = 'public, max-age=60') =>
	Response.json(body, { status, headers: { 'cache-control': cache } });
const refused = (status: number, error: string) => json({ error }, status, 'no-store');

export function fakeProfiles(accounts: FakeAccounts): FakeProfiles {
	/** The own profile of `subject`, if it has a library account. */
	function ownOf(subject: string): OwnProfile | null {
		const account = accounts.accounts.get(subject);
		if (!account) return null;
		const own = fake.own.get(subject);
		if (own) return own;
		const items = structuredClone(fake.everyone);
		// As if every family using one of the items used the most used one.
		const families = Math.max(0, ...items.map((entry) => entry.signals.families));
		return { pseudonym: account.pseudonym, editorial: false, families, items };
	}

	function answer(request: Request): Response {
		const { pathname } = new URL(request.url);
		if (pathname === '/api/account/profile') {
			const bearer = request.headers.get('authorization')?.replace(/^Bearer /, '') ?? '';
			const subject = accounts.sessions.get(bearer);
			if (!subject) return refused(401, 'signed_out');
			const own = ownOf(subject);
			return own ? json(own, 200, 'private, no-store') : refused(404, 'no_account');
		}
		const pseudonym = decodeURIComponent(pathname.slice('/api/authors/'.length)).toLowerCase();
		for (const [subject, account] of accounts.accounts) {
			if (account.pseudonym.toLowerCase() !== pseudonym) continue;
			const own = ownOf(subject);
			if (!own || own.items.length === 0) break;
			const profile: Profile = {
				...own,
				items: own.items.map(
					(entry) =>
						Object.fromEntries(
							Object.entries(entry).filter(([field]) => field !== 'suggestions')
						) as Entry
				)
			};
			return json(profile);
		}
		return refused(404, 'unknown_author');
	}

	const fake: FakeProfiles = {
		own: new Map(),
		everyone: [],
		fetch(request) {
			const { pathname } = new URL(request.url);
			const ours =
				(pathname === '/api/account/profile' || /^\/api\/authors\/[^/]+$/.test(pathname)) &&
				request.method === 'GET';
			return ours ? Promise.resolve(answer(request)) : null;
		}
	};
	return fake;
}
