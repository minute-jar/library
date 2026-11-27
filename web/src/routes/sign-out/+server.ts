// Signs out: the service ends the session and the browser forgets it.

import { redirect } from '@sveltejs/kit';
import { localizeHref } from '#lib/paraglide/runtime.js';
import { SESSION_COOKIE, accounts } from '#lib/server/accounts.ts';
import type { RequestHandler } from './$types';

export const POST: RequestHandler = async ({ fetch, cookies }) => {
	const session = cookies.get(SESSION_COOKIE);
	cookies.delete(SESSION_COOKIE, { path: '/' });
	if (session)
		await accounts(fetch)
			.signOut(session)
			.catch(() => undefined);
	redirect(303, localizeHref('/'));
};
