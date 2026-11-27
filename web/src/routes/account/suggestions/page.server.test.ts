import { describe, expect, it } from 'vitest';
import { SESSION_COOKIE } from '#lib/server/accounts.ts';
import { fakeAccounts, fakeCookies } from '#lib/server/testing/fake-accounts.ts';
import { fakeSuggestions } from '#lib/server/testing/fake-suggestions.ts';
import type { ItemSuggestions } from '#lib/server/suggestions.ts';
import { actions, load } from './+page.server.ts';

type LoadEvent = Parameters<typeof load>[0];
type ActionEvent = Parameters<typeof actions.accept>[0];

const owl = {
	pseudonym: 'Owl',
	minute_jar_account: 'separate' as const,
	opened_at: '2026-11-27T10:00:00Z'
};

const verbs: ItemSuggestions = {
	item: '0123456789abcdef0123456789abcdef',
	title: 'Irregular verbs',
	version: 1,
	questions: [
		{
			question: 1,
			prompt: "Past of 'go'",
			answers: [
				{ answer: 'goed', families: 2 },
				{ answer: 'has gone', families: 1 }
			]
		},
		{ question: 2, prompt: "Past of 'see'", answers: [{ answer: 'seen: past', families: 1 }] }
	]
};

function setUp() {
	const accounts = fakeAccounts();
	const suggestions = fakeSuggestions(accounts);
	return { accounts, suggestions };
}

function event(
	{ accounts, suggestions }: ReturnType<typeof setUp>,
	session?: string,
	form?: [string, string][]
) {
	const fetch = (input: RequestInfo | URL, init?: RequestInit) => {
		const request = new Request(input, init);
		return (
			suggestions.fetch(request) ??
			accounts.fetch(request) ??
			Promise.resolve(new Response(null, { status: 404 }))
		);
	};
	const cookies = fakeCookies(session ? { [SESSION_COOKIE]: session } : {});
	const url = new URL('/account/suggestions', 'https://library.example');
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

describe('the suggestions page', () => {
	it('shows the suggestions of the author’s items, counted by family', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);
		api.suggestions.received.set('subject-1', [verbs]);

		const data = await load(event(api, session) as unknown as LoadEvent);

		expect(data).toEqual({ items: [verbs] });
	});

	it('shares a new version with the answers ticked, for the review', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);
		api.suggestions.received.set('subject-1', [verbs]);

		const answer = await actions.accept(
			event(api, session, [
				['item', verbs.item],
				['accept', '1:goed'],
				['accept', '2:seen: past'],
				['accept', '1:has gone']
			]) as unknown as ActionEvent
		);

		expect(answer).toMatchObject({
			item: verbs.item,
			shared: { outcome: 'published', version: 2 }
		});
		expect(api.suggestions.shared.get('subject-1')).toEqual([
			expect.objectContaining({
				item: verbs.item,
				content: {
					accepted: [
						{ question: 1, answers: ['goed', 'has gone'] },
						{ question: 2, answers: ['seen: past'] }
					]
				}
			})
		]);
	});

	it('asks for at least one answer ticked', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);
		api.suggestions.received.set('subject-1', [verbs]);

		const answer = await actions.accept(
			event(api, session, [['item', verbs.item]]) as unknown as ActionEvent
		);

		expect(answer).toMatchObject({
			status: 400,
			data: { refused: 'answers_required', item: verbs.item }
		});
		expect(api.suggestions.shared.get('subject-1')).toBeUndefined();
	});

	it('says why the service refused the acceptance', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);
		api.suggestions.received.set('subject-1', [verbs]);

		const answer = await actions.accept(
			event(api, session, [
				['item', verbs.item],
				['accept', '1:gone']
			]) as unknown as ActionEvent
		);

		expect(answer).toMatchObject({
			status: 409,
			data: { refused: 'not_suggested', item: verbs.item }
		});
	});

	it('sends back to the account page without a session', async () => {
		const api = setUp();

		const signedOut = await thrown(load(event(api) as unknown as LoadEvent));

		expect(signedOut).toMatchObject({ status: 303, location: '/account' });
	});
});
