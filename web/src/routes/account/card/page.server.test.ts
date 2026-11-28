import { describe, expect, it } from 'vitest';
import { SESSION_COOKIE } from '#lib/server/accounts.ts';
import { fakeAccounts, fakeCookies } from '#lib/server/testing/fake-accounts.ts';
import { fakeCards } from '#lib/server/testing/fake-cards.ts';
import { actions, load } from './+page.server.ts';

type LoadEvent = Parameters<typeof load>[0];
type ActionEvent = Parameters<typeof actions.verify>[0];

const owl = {
	pseudonym: 'Owl',
	minute_jar_account: 'separate' as const,
	opened_at: '2026-11-27T10:00:00Z',
	show_earlier_pseudonyms: false
};
const subscriber = { ...owl, minute_jar_account: 'same_as_subscription' as const };

function setUp() {
	const accounts = fakeAccounts();
	const cards = fakeCards(accounts);
	return { accounts, cards };
}

function event(
	{ accounts, cards }: ReturnType<typeof setUp>,
	session?: string,
	method: 'GET' | 'POST' = 'GET'
) {
	const fetch = (input: RequestInfo | URL, init?: RequestInit) => {
		const request = new Request(input, init);
		return (
			cards.fetch(request) ??
			accounts.fetch(request) ??
			Promise.resolve(new Response(null, { status: 404 }))
		);
	};
	const cookies = fakeCookies(session ? { [SESSION_COOKIE]: session } : {});
	const url = new URL('/account/card', 'https://library.example');
	const request = new Request(url, { method, body: method === 'POST' ? '' : undefined });
	return { url, fetch, cookies: cookies.cookies, request, written: cookies.written };
}

async function thrown(answer: unknown) {
	try {
		await answer;
	} catch (thrown) {
		return thrown as { status: number; location?: string };
	}
	throw new Error('nothing thrown');
}

describe('the card page', () => {
	it('offers a zero-amount check to an account without a verified card', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);

		const data = await load(event(api, session) as unknown as LoadEvent);

		expect(data).toEqual({
			card: null,
			subscription: false,
			strong: true,
			outcome: null
		});
	});

	it('says the subscription card is used when the account is on the subscription', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', subscriber);

		const data = await load(event(api, session) as unknown as LoadEvent);

		expect(data.subscription).toBe(true);
	});

	it('sends a parent without a subscription to the provider for a zero-amount check', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);

		const to = await thrown(actions.verify(event(api, session, 'POST') as unknown as ActionEvent));

		expect(to).toEqual(
			expect.objectContaining({ status: 303, location: 'https://pay.example/setup/seti_1' })
		);
	});

	it('uses the subscription card at once', async () => {
		const api = setUp();
		api.cards.subscribers.add('subject-1');
		const session = api.accounts.signedIn('subject-1', subscriber);

		const answer = await actions.verify(event(api, session, 'POST') as unknown as ActionEvent);
		const data = await load(event(api, session) as unknown as LoadEvent);

		expect(answer).toEqual({ verified: true });
		expect(data.card).toEqual({ verified_by: 'subscription', verified_at: '2026-11-27T10:00:00Z' });
	});

	it('shows the card verified once the browser is back from the provider', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);
		await thrown(actions.verify(event(api, session, 'POST') as unknown as ActionEvent));
		api.cards.checks.set('seti_1', 'verified');

		const data = await load(event(api, session) as unknown as LoadEvent);

		expect(data.card).toEqual({ verified_by: 'check', verified_at: '2026-11-27T10:02:00Z' });
		expect(data.outcome).toBeNull();
	});

	it('says when the check is not finished or was refused', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);
		await thrown(actions.verify(event(api, session, 'POST') as unknown as ActionEvent));

		const pending = await load(event(api, session) as unknown as LoadEvent);
		api.cards.checks.set('seti_1', 'refused');
		const refused = await load(event(api, session) as unknown as LoadEvent);

		expect(pending).toMatchObject({ card: null, outcome: 'card_check_pending' });
		expect(refused).toMatchObject({ card: null, outcome: 'card_refused' });
	});

	it('asks for a strong sign-in again when the last one is not recent', async () => {
		const api = setUp();
		api.accounts.strong = false;
		const session = api.accounts.signedIn('subject-1', owl);

		const data = await load(event(api, session) as unknown as LoadEvent);
		const answer = await actions.verify(event(api, session, 'POST') as unknown as ActionEvent);

		expect(data.strong).toBe(false);
		expect(answer).toMatchObject({ status: 403, data: { refused: 'strong_sign_in_required' } });
	});

	it('sends back to the account page without a library account or a session', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1');

		const withoutAccount = await thrown(load(event(api, session) as unknown as LoadEvent));
		const signedOut = await thrown(load(event(api) as unknown as LoadEvent));

		expect(withoutAccount).toMatchObject({ status: 303, location: '/account' });
		expect(signedOut).toMatchObject({ status: 303, location: '/account' });
	});
});
