import { describe, expect, it } from 'vitest';
import { SESSION_COOKIE } from '#lib/server/accounts.ts';
import { fakeAccounts, fakeCookies } from '#lib/server/testing/fake-accounts.ts';
import { TERMS, fakeShares } from '#lib/server/testing/fake-shares.ts';
import type { AuthorNotification, Share } from '#lib/server/shares.ts';
import { actions, load } from './+page.server.ts';

type LoadEvent = Parameters<typeof load>[0];
type ActionEvent = Parameters<typeof actions.accept>[0];
type AppealEvent = Parameters<typeof actions.appeal>[0];

const owl = {
	pseudonym: 'Owl',
	minute_jar_account: 'separate' as const,
	opened_at: '2026-11-27T10:00:00Z',
	show_earlier_pseudonyms: false
};

const refusedShare: Share = {
	share: '11111111111111111111111111111111',
	item: null,
	title: 'Irregular verbs',
	outcome: 'refused',
	version: null,
	reasons: [{ question: 2, reason: 'wrong_answer', explanation: "The past of 'see' is 'saw'." }],
	shared_at: '2026-11-27T10:01:00Z'
};

function setUp() {
	const accounts = fakeAccounts();
	const shares = fakeShares(accounts);
	return { accounts, shares };
}

function event(
	{ accounts, shares }: ReturnType<typeof setUp>,
	session?: string,
	form?: Record<string, string>
) {
	const fetch = (input: RequestInfo | URL, init?: RequestInit) => {
		const request = new Request(input, init);
		return (
			shares.fetch(request) ??
			accounts.fetch(request) ??
			Promise.resolve(new Response(null, { status: 404 }))
		);
	};
	const cookies = fakeCookies(session ? { [SESSION_COOKIE]: session } : {});
	const url = new URL('/account/sharing', 'https://library.example');
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

describe('the sharing page', () => {
	it('shows the terms to accept before the first share, and the whole quota', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);

		const data = await load(event(api, session) as unknown as LoadEvent);

		expect(data).toEqual({
			sharing_terms: { current: TERMS, accepted: null },
			quota: { shares_per_day: 10, shared_today: 0, next_share_at: null },
			shares: [],
			notifications: []
		});
	});

	it('accepts the sharing terms once the box is ticked', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);

		const unticked = await actions.accept(
			event(api, session, { version: TERMS }) as unknown as ActionEvent
		);
		const ticked = await actions.accept(
			event(api, session, { version: TERMS, accepted: 'on' }) as unknown as ActionEvent
		);
		const data = await load(event(api, session) as unknown as LoadEvent);

		expect(unticked).toMatchObject({ status: 400, data: { refused: 'acceptance_required' } });
		expect(ticked).toEqual({ accepted: true });
		expect(data.sharing_terms).toEqual({ current: TERMS, accepted: TERMS });
	});

	it('says when the terms shown have changed meanwhile', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);

		const answer = await actions.accept(
			event(api, session, { version: '2020-01', accepted: 'on' }) as unknown as ActionEvent
		);

		expect(answer).toMatchObject({ status: 409, data: { refused: 'outdated_sharing_terms' } });
	});

	it('lists every share with its outcome and the reasons of a refusal', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);
		api.shares.shares.set('subject-1', [refusedShare]);

		const data = await load(event(api, session) as unknown as LoadEvent);

		expect(data.shares).toEqual([refusedShare]);
	});

	it('explains the quota once reached', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);
		api.shares.today.set('subject-1', 10);

		const data = await load(event(api, session) as unknown as LoadEvent);

		expect(data.quota).toEqual({
			shares_per_day: 10,
			shared_today: 10,
			next_share_at: '2026-11-28T00:00:00Z'
		});
	});

	it('tells the author of decisions on their items and takes their appeal', async () => {
		const api = setUp();
		const session = api.accounts.signedIn('subject-1', owl);
		const told: AuthorNotification = {
			item: '0123456789abcdef0123456789abcdef',
			title: 'Irregular verbs',
			report: '22222222222222222222222222222222',
			reason: 'other',
			details: 'Question 1',
			decision: { outcome: 'upheld', reasons: 'Off topic.' },
			appeal: null,
			at: '2026-11-27T10:00:00Z'
		};
		api.shares.notifications.set('subject-1', [told]);

		const data = await load(event(api, session) as unknown as LoadEvent);
		const empty = await actions.appeal(
			event(api, session, { report: told.report, statement: ' ' }) as unknown as AppealEvent
		);
		const appealed = await actions.appeal(
			event(api, session, {
				report: told.report,
				statement: 'Every question is about verbs.'
			}) as unknown as AppealEvent
		);

		expect(data.notifications).toEqual([told]);
		expect(empty).toMatchObject({ status: 400, data: { refused: 'statement_required' } });
		expect(appealed).toEqual({ appealed: told.report });
		expect(api.shares.appeals.get(told.report)).toBe('Every question is about verbs.');
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
