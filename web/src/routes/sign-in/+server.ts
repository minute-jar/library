// Starts a sign-in with a Minute Jar account: the browser goes to the
// identity provider, with the state kept in a cookie to check when it comes
// back. `?strong` asks for a new strong sign-in (accounts-api.md).

import { error, redirect } from '@sveltejs/kit';
import { getLocale } from '#lib/paraglide/runtime.js';
import { SIGN_IN_COOKIE, accounts } from '#lib/server/accounts.ts';
import type { RequestHandler } from './$types';

export const GET: RequestHandler = async ({ url, fetch, cookies }) => {
	let started: { state: string; authorization_url: string };
	try {
		started = await accounts(fetch).startSignIn(url.searchParams.has('strong'));
	} catch {
		error(503, 'Sign-in is unavailable');
	}
	cookies.set(SIGN_IN_COOKIE, `${started.state}.${getLocale()}`, {
		httpOnly: true,
		sameSite: 'lax',
		path: '/',
		maxAge: 600,
		secure: url.protocol === 'https:'
	});
	// The identity provider's address comes from the service's own configuration.
	redirect(303, started.authorization_url, { external: true });
};
