import { describe, expect, it } from 'vitest';
import { SESSION_COOKIE } from '#lib/server/accounts.ts';
import { fakeAccounts, fakeCookies } from '#lib/server/testing/fake-accounts.ts';
import { fakeAuthored } from '#lib/server/testing/fake-authored.ts';
import type { AuthoredItem } from '#lib/server/authored.ts';
import { actions, load } from './+page.server.ts';

type LoadEvent = Parameters<typeof load>[0];
type WithdrawEvent = Parameters<typeof actions.withdraw>[0];

const owl = {
	pseudonym: 'Owl',
	minute_jar_account: 'separate' as const,
	opened_at: '2026-11-27T10:00:00Z',
	show_earlier_pseudonyms: false
};

const verbs: AuthoredItem = {
	item: '0123456789abcdef0123456789abcdef',
	title: 'Irregular verbs, part 2',
	withdrawal: null,
	versions: [
		{ version: 2, published_at: '2026-11-27T10:05:00Z', withdrawal: null },
		{ version: 1, published_at: '2026-11-27T10:00:00Z', withdrawal: null }
	]
};

function setUp() {
	const accounts = fakeAccounts();
	const authored = fakeAuthored(accounts);
	return { accounts, authored };
}

function event(
	{ accounts, authored }: ReturnType<typeof setUp>,
	session?: string,
	form?: Record<string, string>
) {
	const fetch = (input: RequestInfo | URL, init?: RequestInit) => {
		const request = new Request(input, init);
		return (
			authored.fetch(request) ??
			accounts.fetch(request) ??
			Promise.resolve(new Response(null, { status: 404 }))
		);
	};
	const cookies = fakeCookies(session ? { [SESSION_COOKIE]: session } : {});
	const url = new URL('/account/items', 'https://library.example');
	const request = form
		? new Request(url, { method: 'POST', body: new URLSearchParams(form) })
		: new Request(url);
	return { url, fetch, cookies: cookies.cookies, request };
}

async function thrown(answer: unknown) {
	try {
		await answer;
	} catch (thrown) {
		return thrown as { status: number; location?: string };
	}
	throw new Error('nothing thrown');
}

describe("the author's items page", () => {
	it('shows the items with their versions, and whether the sign-in is strong', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);
		api.authored.items.set('subject-1', [structuredClone(verbs)]);

		const data = await load(event(api, session) as unknown as LoadEvent);

		expect(data).toEqual({ items: [verbs], strong: true });
	});

	it('sends a visitor without a session to the account page', async () => {
		const api = setUp();

		const answer = await thrown(load(event(api) as unknown as LoadEvent));

		expect(answer).toMatchObject({ status: 303, location: '/account' });
	});

	it('withdraws the whole item once the author confirms', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);
		api.authored.items.set('subject-1', [structuredClone(verbs)]);

		const unconfirmed = await actions.withdraw(
			event(api, session, { item: verbs.item, version: '' }) as unknown as WithdrawEvent
		);
		const confirmed = await actions.withdraw(
			event(api, session, {
				item: verbs.item,
				version: '',
				understood: 'on'
			}) as unknown as WithdrawEvent
		);

		expect(unconfirmed).toMatchObject({
			status: 400,
			data: { refused: 'confirmation_required', item: verbs.item }
		});
		expect(confirmed).toEqual({ withdrawn: verbs.item });
		expect(api.authored.withdrawn).toEqual([{ item: verbs.item, version: null }]);
		const data = await load(event(api, session) as unknown as LoadEvent);
		expect(data.items[0].withdrawal).toMatchObject({ kind: 'author', reason: 'author_choice' });
	});

	it('withdraws one version', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);
		api.authored.items.set('subject-1', [structuredClone(verbs)]);

		await actions.withdraw(
			event(api, session, {
				item: verbs.item,
				version: '2',
				understood: 'on'
			}) as unknown as WithdrawEvent
		);

		expect(api.authored.withdrawn).toEqual([{ item: verbs.item, version: 2 }]);
		const data = await load(event(api, session) as unknown as LoadEvent);
		expect(data.items[0].withdrawal).toBeNull();
		expect(data.items[0].versions[0].withdrawal).toMatchObject({ kind: 'author' });
	});

	it('asks for a strong sign-in again when the last one is not recent', async () => {
		const api = setUp();
		api.accounts.strong = false;
		const session = api.accounts.signedIn('subject-1', owl);
		api.authored.items.set('subject-1', [structuredClone(verbs)]);

		const data = await load(event(api, session) as unknown as LoadEvent);
		const refused = await actions.withdraw(
			event(api, session, {
				item: verbs.item,
				version: '',
				understood: 'on'
			}) as unknown as WithdrawEvent
		);

		expect(data.strong).toBe(false);
		expect(refused).toMatchObject({
			status: 403,
			data: { refused: 'strong_sign_in_required', item: verbs.item }
		});
		expect(api.authored.withdrawn).toEqual([]);
	});

	it("refuses another account's item", async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);
		api.authored.items.set('subject-2', [structuredClone(verbs)]);

		const refused = await actions.withdraw(
			event(api, session, {
				item: verbs.item,
				version: '',
				understood: 'on'
			}) as unknown as WithdrawEvent
		);

		expect(refused).toMatchObject({ status: 404, data: { refused: 'unknown_item' } });
		expect(api.authored.withdrawn).toEqual([]);
	});
});
