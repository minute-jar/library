import { describe, expect, it } from 'vitest';
import { SESSION_COOKIE, SIGN_IN_COOKIE } from '#lib/server/accounts.ts';
import { fakeAccounts, fakeCookies } from '#lib/server/testing/fake-accounts.ts';
import { GET as signIn } from './+server.ts';
import { GET as callback } from './callback/+server.ts';
import { POST as signOut } from '../sign-out/+server.ts';

type Event = Parameters<typeof signIn>[0] & Parameters<typeof callback>[0];

function event(path: string, api = fakeAccounts(), cookies = fakeCookies()) {
	const fetch = (input: RequestInfo | URL, init?: RequestInit) =>
		api.fetch(new Request(input, init)) ?? Promise.resolve(new Response(null, { status: 404 }));
	const url = new URL(path, 'https://library.example');
	return { event: { url, fetch, cookies: cookies.cookies } as unknown as Event, api, cookies };
}

async function redirected(answer: unknown) {
	try {
		await answer;
	} catch (thrown) {
		return thrown as { status: number; location: string };
	}
	throw new Error('no redirect');
}

describe('signing in', () => {
	it('sends the browser to the identity provider and keeps the state in a cookie', async () => {
		const { event: start, api, cookies } = event('/sign-in');

		const to = await redirected(signIn(start));

		expect(to).toMatchObject({ status: 303 });
		expect(to.location).toBe('https://id.example/authorize?state=state-1&strong=false');
		expect(api.asked[0]).toMatchObject({
			method: 'POST',
			path: '/api/sign-in',
			body: { strong: false }
		});
		expect(cookies.written[SIGN_IN_COOKIE]).toMatchObject({
			value: 'state-1.en',
			options: { httpOnly: true, sameSite: 'lax', path: '/', maxAge: 600, secure: true }
		});
	});

	it('asks for a strong sign-in when the page needs one', async () => {
		const { event: start, api } = event('/sign-in?strong');

		const to = await redirected(signIn(start));

		expect(to.location).toContain('strong=true');
		expect(api.asked[0].body).toEqual({ strong: true });
	});

	it('opens the session when the provider comes back with the state it was given', async () => {
		const api = fakeAccounts();
		const cookies = fakeCookies({ [SIGN_IN_COOKIE]: 'state-1.fr' });
		await api.fetch(new Request('https://api.example/api/sign-in', { method: 'POST', body: '{}' }));

		const to = await redirected(
			callback(event('/sign-in/callback?code=code-state-1&state=state-1', api, cookies).event)
		);

		expect(to).toEqual(expect.objectContaining({ status: 303, location: '/fr/account' }));
		expect(cookies.written[SESSION_COOKIE]).toMatchObject({
			value: 'session-2',
			options: { httpOnly: true, sameSite: 'lax', path: '/', secure: true }
		});
		expect(cookies.written[SESSION_COOKIE].options.expires).toEqual(
			new Date('2026-12-27T10:00:00Z')
		);
		expect(cookies.written[SIGN_IN_COOKIE].options.deleted).toBe(true);
	});

	it('refuses a callback whose state is not the one this browser started', async () => {
		const api = fakeAccounts();
		const cookies = fakeCookies({ [SIGN_IN_COOKIE]: 'state-mine.en' });

		const to = await redirected(
			callback(event('/sign-in/callback?code=code-state-1&state=state-1', api, cookies).event)
		);

		expect(to.location).toBe('/account?sign_in=failed');
		expect(api.asked).toEqual([]);
		expect(cookies.written[SESSION_COOKIE]).toBeUndefined();
	});

	it('says the sign-in failed when the provider or the service refuses it', async () => {
		const cookies = fakeCookies({ [SIGN_IN_COOKIE]: 'state-1.en' });
		const refused = await redirected(
			callback(
				event('/sign-in/callback?error=access_denied&state=state-1', undefined, cookies).event
			)
		);
		const unknown = await redirected(
			callback(
				event(
					'/sign-in/callback?code=code-state-1&state=state-1',
					undefined,
					fakeCookies({ [SIGN_IN_COOKIE]: 'state-1.en' })
				).event
			)
		);

		expect(refused.location).toBe('/account?sign_in=failed');
		expect(unknown.location).toBe('/account?sign_in=failed');
	});

	it('comes back to the moderation queue when a moderator signs in again from it', async () => {
		const { event: start, cookies: started } = event('/sign-in?strong&return=moderation');
		await redirected(signIn(start));
		const api = fakeAccounts();
		const cookies = fakeCookies({ [SIGN_IN_COOKIE]: 'state-1.fr.moderation' });
		await api.fetch(new Request('https://api.example/api/sign-in', { method: 'POST', body: '{}' }));

		const to = await redirected(
			callback(event('/sign-in/callback?code=code-state-1&state=state-1', api, cookies).event)
		);

		expect(started.written[SIGN_IN_COOKIE].value).toBe('state-1.en.moderation');
		expect(to.location).toBe('/fr/moderation');
	});

	it('comes back to the account page for any other return', async () => {
		const { event: start, cookies } = event('/sign-in?return=https://elsewhere.example');

		await redirected(signIn(start));

		expect(cookies.written[SIGN_IN_COOKIE].value).toBe('state-1.en');
	});

	it('signs out at the service and forgets the session', async () => {
		const api = fakeAccounts();
		const session = api.signedIn('subject-1');
		const cookies = fakeCookies({ [SESSION_COOKIE]: session });

		const to = await redirected(signOut(event('/sign-out', api, cookies).event));

		expect(to).toMatchObject({ status: 303, location: '/' });
		expect(api.sessions.has(session)).toBe(false);
		expect(cookies.written[SESSION_COOKIE].options.deleted).toBe(true);
	});
});
