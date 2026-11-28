// The author's items (story S17): every item the library account published,
// its versions and what withdrew them, and the author's withdrawal of an
// item or of one version, for good, after a recent strong sign-in.

import { error, fail, redirect } from '@sveltejs/kit';
import { localizeHref } from '#lib/paraglide/runtime.js';
import { Unavailable } from '#lib/server/catalogue.ts';
import { Refused, SESSION_COOKIE, signedOut, accounts } from '#lib/server/accounts.ts';
import { type AuthoredItem, authored } from '#lib/server/authored.ts';
import type { Actions, PageServerLoad } from './$types';

export const load = (async ({ fetch, cookies }) => {
	const session = cookies.get(SESSION_COOKIE);
	if (!session) redirect(303, localizeHref('/account'));
	let items: AuthoredItem[];
	let strong: boolean;
	try {
		const state = await accounts(fetch).session(session);
		if (!state?.account) redirect(303, localizeHref('/account'));
		strong = state.strong_sign_in_until !== null;
		items = await authored(fetch).items(session);
	} catch (failure) {
		if (failure instanceof Refused) redirect(303, localizeHref('/account'));
		if (failure instanceof Unavailable) error(503, 'Your items are unavailable');
		throw failure;
	}
	return { items, strong };
}) satisfies PageServerLoad;

export const actions = {
	withdraw: async ({ fetch, cookies, request }) => {
		const session = cookies.get(SESSION_COOKIE);
		if (!session) return signedOut({ item: '' });
		const form = await request.formData();
		const item = form.get('item');
		const version = form.get('version');
		if (typeof item !== 'string' || typeof version !== 'string' || !/^\d*$/.test(version)) {
			return fail(400, { refused: 'bad_request', item: '' });
		}
		if (form.get('understood') !== 'on') {
			return fail(400, { refused: 'confirmation_required', item });
		}
		try {
			await authored(fetch).withdraw(session, item, version === '' ? null : Number(version));
		} catch (failure) {
			if (failure instanceof Refused) return fail(failure.status, { refused: failure.code, item });
			if (failure instanceof Unavailable) error(503, 'Your items are unavailable');
			throw failure;
		}
		return { withdrawn: item };
	}
} satisfies Actions;
