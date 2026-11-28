import { describe, expect, it } from 'vitest';
import { SESSION_COOKIE } from '#lib/server/accounts.ts';
import { fakeAccounts, fakeCookies } from '#lib/server/testing/fake-accounts.ts';
import { fakeProfiles } from '#lib/server/testing/fake-profiles.ts';
import { verbs } from '#lib/server/testing/fake-api.ts';
import type { OwnProfile } from '#lib/server/profiles.ts';
import { load } from './+page.server.ts';

type LoadEvent = Parameters<typeof load>[0];

const owl = {
	pseudonym: 'Owl',
	minute_jar_account: 'separate' as const,
	opened_at: '2026-11-27T10:00:00Z'
};

const ownProfile: OwnProfile = {
	pseudonym: 'Owl',
	editorial: false,
	families: 3,
	items: [
		{
			...verbs,
			signals: { families: 3, ratings: [0, 0, 0, 0, 1] },
			suggestions: { families: 2, answers: 3 }
		}
	]
};

function setUp() {
	const accounts = fakeAccounts();
	const profiles = fakeProfiles(accounts);
	return { accounts, profiles };
}

function event(
	{ accounts, profiles }: ReturnType<typeof setUp>,
	pseudonym: string,
	session?: string
) {
	const fetch = (input: RequestInfo | URL, init?: RequestInit) => {
		const request = new Request(input, init);
		return (
			profiles.fetch(request) ??
			accounts.fetch(request) ??
			Promise.resolve(new Response(null, { status: 404 }))
		);
	};
	const cookies = fakeCookies(session ? { [SESSION_COOKIE]: session } : {});
	return { params: { pseudonym }, fetch, cookies: cookies.cookies };
}

async function thrown(answer: unknown) {
	try {
		await answer;
	} catch (thrown) {
		return thrown as { status: number };
	}
	throw new Error('nothing thrown');
}

describe('an author’s profile', () => {
	it('shows anyone the author’s items and families, without the suggestions received', async () => {
		const api = setUp();
		api.accounts.signedIn('subject-1', owl);
		api.profiles.own.set('subject-1', ownProfile);

		const data = await load(event(api, 'owl') as unknown as LoadEvent);

		expect(data.own).toBe(false);
		expect(data.profile.pseudonym).toBe('Owl');
		expect(data.profile.families).toBe(3);
		expect(data.profile.items.map((entry) => entry.item)).toEqual([verbs.item]);
		expect(data.profile.items[0]).not.toHaveProperty('suggestions');
	});

	it('shows the author their own profile with the suggestions received', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);
		api.profiles.own.set('subject-1', ownProfile);

		const data = await load(event(api, 'Owl', session) as unknown as LoadEvent);

		expect(data).toEqual({ profile: ownProfile, own: true });
	});

	it('shows the author their own profile before they share anything', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);
		const empty = { ...ownProfile, families: 0, items: [] };
		api.profiles.own.set('subject-1', empty);

		const data = await load(event(api, 'Owl', session) as unknown as LoadEvent);

		expect(data).toEqual({ profile: empty, own: true });
	});

	it('shows another author’s profile as anyone sees it to a signed-in author', async () => {
		const api = setUp();
		api.accounts.signedIn('subject-1', owl);
		api.profiles.own.set('subject-1', ownProfile);
		const lark = api.accounts.signedIn('subject-2', { ...owl, pseudonym: 'Lark' });

		const data = await load(event(api, 'Owl', lark) as unknown as LoadEvent);

		expect(data.own).toBe(false);
		expect(data.profile.items[0]).not.toHaveProperty('suggestions');
	});

	it('answers 404 for a pseudonym that is no author’s', async () => {
		const api = setUp();

		expect((await thrown(load(event(api, 'Nobody') as unknown as LoadEvent))).status).toBe(404);
	});
});
