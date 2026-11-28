import { describe, expect, it } from 'vitest';
import { SESSION_COOKIE } from '#lib/server/accounts.ts';
import { fakeAccounts, fakeCookies } from '#lib/server/testing/fake-accounts.ts';
import { actions, load } from './+page.server.ts';

type LoadEvent = Parameters<typeof load>[0];
type ActionEvent = Parameters<typeof actions.open>[0];

const owl = {
	pseudonym: 'Owl',
	minute_jar_account: 'separate' as const,
	opened_at: '2026-11-27T10:00:00Z',
	show_earlier_pseudonyms: false
};

function event(
	path: string,
	api = fakeAccounts(),
	session?: string,
	form?: Record<string, string>
) {
	const fetch = (input: RequestInfo | URL, init?: RequestInit) =>
		api.fetch(new Request(input, init)) ?? Promise.resolve(new Response(null, { status: 404 }));
	const cookies = fakeCookies(session ? { [SESSION_COOKIE]: session } : {});
	const url = new URL(path, 'https://library.example');
	const request = new Request(url, {
		method: form ? 'POST' : 'GET',
		body: form ? new URLSearchParams(form) : undefined
	});
	return { url, fetch, cookies: cookies.cookies, request, written: cookies.written };
}

const opening = {
	pseudonym: 'Owl',
	adult_and_parent: 'on',
	minute_jar_account: 'separate'
};

describe('the library account page', () => {
	it('offers to sign in when signed out', async () => {
		const data = await load(event('/account') as unknown as LoadEvent);

		expect(data).toEqual({ signedIn: false, signInFailed: false, account: null, strong: false });
	});

	it('says when a sign-in failed', async () => {
		const data = await load(event('/account?sign_in=failed') as unknown as LoadEvent);

		expect(data.signInFailed).toBe(true);
	});

	it('forgets a session the service no longer knows', async () => {
		const page = event('/account', fakeAccounts(), 'session-ended');

		const data = await load(page as unknown as LoadEvent);

		expect(data.signedIn).toBe(false);
		expect(page.written[SESSION_COOKIE].options.deleted).toBe(true);
	});

	it('offers to open an account after a recent strong sign-in', async () => {
		const api = fakeAccounts();
		const session = api.signedIn('subject-1');

		const data = await load(event('/account', api, session) as unknown as LoadEvent);

		expect(data).toEqual({ signedIn: true, signInFailed: false, account: null, strong: true });
		expect(api.asked).toEqual([{ method: 'GET', path: '/api/session', session, body: undefined }]);
	});

	it('asks for a strong sign-in again when the last one is not recent', async () => {
		const api = fakeAccounts();
		api.strong = false;
		const session = api.signedIn('subject-1');

		const data = await load(event('/account', api, session) as unknown as LoadEvent);

		expect(data.strong).toBe(false);
	});

	it('shows the library account once opened', async () => {
		const api = fakeAccounts();
		const session = api.signedIn('subject-1', owl);

		const data = await load(event('/account', api, session) as unknown as LoadEvent);

		expect(data.account).toEqual(owl);
	});

	it('opens the account with the pseudonym, the statement and the choice', async () => {
		const api = fakeAccounts();
		const session = api.signedIn('subject-1');

		const answer = await actions.open(
			event('/account', api, session, opening) as unknown as ActionEvent
		);

		expect(answer).toEqual({ opened: true });
		expect(api.asked.at(-1)).toEqual({
			method: 'POST',
			path: '/api/account',
			session,
			body: { pseudonym: 'Owl', adult_and_parent: true, minute_jar_account: 'separate' }
		});
	});

	it('sends no statement when the box is left unticked, and says why it is refused', async () => {
		const api = fakeAccounts();
		const session = api.signedIn('subject-1');
		const unticked = { pseudonym: 'Owl', minute_jar_account: 'same_as_subscription' };

		const answer = await actions.open(
			event('/account', api, session, unticked) as unknown as ActionEvent
		);

		expect(answer).toMatchObject({
			status: 400,
			data: { refused: 'statement_required', pseudonym: 'Owl' }
		});
		expect(api.asked.at(-1)?.body).toMatchObject({ adult_and_parent: false });
	});

	it('refuses a choice it does not know without asking the service', async () => {
		const api = fakeAccounts();
		const session = api.signedIn('subject-1');

		const answer = await actions.open(
			event('/account', api, session, {
				...opening,
				minute_jar_account: 'both'
			}) as unknown as ActionEvent
		);

		expect(answer).toMatchObject({ status: 400, data: { refused: 'choice_required' } });
		expect(api.asked).toEqual([]);
	});

	it('passes on the refusal of a sign-in no longer strong', async () => {
		const api = fakeAccounts();
		api.strong = false;
		const session = api.signedIn('subject-1');

		const answer = await actions.open(
			event('/account', api, session, opening) as unknown as ActionEvent
		);

		expect(answer).toMatchObject({ status: 403, data: { refused: 'strong_sign_in_required' } });
	});

	it('refuses to open an account when signed out', async () => {
		const answer = await actions.open(
			event('/account', fakeAccounts(), undefined, opening) as unknown as ActionEvent
		);

		expect(answer).toMatchObject({ status: 401, data: { refused: 'signed_out' } });
	});

	it('changes the pseudonym', async () => {
		const api = fakeAccounts();
		const session = api.signedIn('subject-1', owl);

		const answer = await actions.pseudonym(
			event('/account', api, session, { pseudonym: 'Barn owl' }) as unknown as ActionEvent
		);
		const refused = await actions.pseudonym(
			event('/account', api, session, { pseudonym: 'a@b' }) as unknown as ActionEvent
		);

		expect(answer).toEqual({ renamed: true });
		expect(api.accounts.get('subject-1')?.pseudonym).toBe('Barn owl');
		expect(refused).toMatchObject({ status: 400, data: { refused: 'bad_pseudonym' } });
	});

	it('shows the items of earlier pseudonyms on the public profile only when chosen', async () => {
		const api = fakeAccounts();
		const session = api.signedIn('subject-1', owl);

		const shown = await actions.earlier(
			event('/account', api, session, {
				show_earlier_pseudonyms: 'on'
			}) as unknown as ActionEvent
		);
		expect(shown).toEqual({ earlier: true });
		expect(api.accounts.get('subject-1')?.show_earlier_pseudonyms).toBe(true);

		const hidden = await actions.earlier(
			event('/account', api, session, {}) as unknown as ActionEvent
		);
		expect(hidden).toEqual({ earlier: false });
		expect(api.accounts.get('subject-1')?.show_earlier_pseudonyms).toBe(false);
		const signedOut = await actions.earlier(
			event('/account', api, undefined, {}) as unknown as ActionEvent
		);
		expect(signedOut).toMatchObject({ status: 401, data: { refused: 'signed_out' } });
	});

	it('closes the account once the parent confirms, and ends the session', async () => {
		const api = fakeAccounts();
		const session = api.signedIn('subject-1', owl);
		const page = event('/account', api, session, { understood: 'on' });

		const answer = await actions.close(page as unknown as ActionEvent);

		expect(answer).toEqual({ closed: true });
		expect(api.asked.at(-1)).toEqual({
			method: 'DELETE',
			path: '/api/account',
			session,
			body: undefined
		});
		expect(page.written[SESSION_COOKIE].options.deleted).toBe(true);
		expect(api.sessions.has(session)).toBe(false);
	});

	it('does not close the account unless the parent confirms', async () => {
		const api = fakeAccounts();
		const session = api.signedIn('subject-1', owl);

		const answer = await actions.close(
			event('/account', api, session, {}) as unknown as ActionEvent
		);

		expect(answer).toMatchObject({ status: 400, data: { refused: 'confirmation_required' } });
		expect(api.asked).toEqual([]);
	});

	it('asks for a strong sign-in again before closing', async () => {
		const api = fakeAccounts();
		api.strong = false;
		const session = api.signedIn('subject-1', owl);
		const page = event('/account', api, session, { understood: 'on' });

		const answer = await actions.close(page as unknown as ActionEvent);

		expect(answer).toMatchObject({ status: 403, data: { refused: 'strong_sign_in_required' } });
		expect(api.accounts.get('subject-1')).toEqual(owl);
		expect(page.written[SESSION_COOKIE]).toBeUndefined();
	});

	it('keeps the pseudonym of a closed account from others', async () => {
		const api = fakeAccounts();
		const session = api.signedIn('subject-1', owl);
		await actions.close(
			event('/account', api, session, { understood: 'on' }) as unknown as ActionEvent
		);
		const other = api.signedIn('subject-2');

		const answer = await actions.open(
			event('/account', api, other, opening) as unknown as ActionEvent
		);

		expect(answer).toMatchObject({ status: 409, data: { refused: 'pseudonym_taken' } });
	});
});
