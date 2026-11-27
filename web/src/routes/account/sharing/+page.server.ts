// Sharing from a library account (stories S10, S13, S16): the sharing terms
// in plain words, accepted before the first share; the outcome of every
// share, with the review's reasons per question; and the day's quota.

import { error, fail, redirect } from '@sveltejs/kit';
import { localizeHref } from '#lib/paraglide/runtime.js';
import { Unavailable } from '#lib/server/catalogue.ts';
import { Refused, SESSION_COOKIE } from '#lib/server/accounts.ts';
import { type Standing, shares } from '#lib/server/shares.ts';
import type { Actions, PageServerLoad } from './$types';

export const load = (async ({ fetch, cookies }) => {
	const session = cookies.get(SESSION_COOKIE);
	if (!session) redirect(303, localizeHref('/account'));
	let standing: Standing;
	try {
		standing = await shares(fetch).standing(session);
	} catch (failure) {
		if (failure instanceof Refused) redirect(303, localizeHref('/account'));
		if (failure instanceof Unavailable) error(503, 'Sharing is unavailable');
		throw failure;
	}
	return standing;
}) satisfies PageServerLoad;

export const actions = {
	accept: async ({ fetch, cookies, request }) => {
		const session = cookies.get(SESSION_COOKIE);
		if (!session) return fail(401, { refused: 'signed_out' });
		const form = await request.formData();
		const version = form.get('version');
		if (form.get('accepted') !== 'on' || typeof version !== 'string') {
			return fail(400, { refused: 'acceptance_required' });
		}
		try {
			await shares(fetch).accept(session, version);
		} catch (failure) {
			if (failure instanceof Refused) return fail(failure.status, { refused: failure.code });
			if (failure instanceof Unavailable) error(503, 'Sharing is unavailable');
			throw failure;
		}
		return { accepted: true };
	}
} satisfies Actions;
