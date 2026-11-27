// Where the identity provider sends the browser back: the state must be the
// one this browser started, then the service opens the session, kept in an
// HttpOnly cookie that the browser's scripts cannot read.

import { redirect } from '@sveltejs/kit';
import { type Locale, isLocale, localizeHref } from '#lib/paraglide/runtime.js';
import { SESSION_COOKIE, SIGN_IN_COOKIE, accounts } from '#lib/server/accounts.ts';
import type { RequestHandler } from './$types';

export const GET: RequestHandler = async ({ url, fetch, cookies }) => {
	const [state, language] = (cookies.get(SIGN_IN_COOKIE) ?? '').split('.');
	const locale: Locale = isLocale(language) ? language : 'en';
	cookies.delete(SIGN_IN_COOKIE, { path: '/' });
	const code = url.searchParams.get('code');
	const failed = localizeHref('/account?sign_in=failed', { locale });
	if (!state || url.searchParams.get('state') !== state || !code) redirect(303, failed);

	let session: { session: string; expires_at: string };
	try {
		session = await accounts(fetch).finishSignIn(state, code);
	} catch {
		redirect(303, failed);
	}
	cookies.set(SESSION_COOKIE, session.session, {
		httpOnly: true,
		sameSite: 'lax',
		path: '/',
		expires: new Date(session.expires_at),
		secure: url.protocol === 'https:'
	});
	redirect(303, localizeHref('/account', { locale }));
};
