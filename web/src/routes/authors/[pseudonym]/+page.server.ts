// An author's profile (story S19): their items and how many families use
// them, the same for everyone. The author, signed in, sees their own with
// the suggestions received, which no one else sees, even before their
// first item shows.

import { error } from '@sveltejs/kit';
import { Refused, SESSION_COOKIE } from '#lib/server/accounts.ts';
import { Unavailable } from '#lib/server/catalogue.ts';
import { NotFound } from '#lib/server/items.ts';
import { profiles } from '#lib/server/profiles.ts';
import type { PageServerLoad } from './$types';

export const load = (async ({ params, fetch, cookies }) => {
	const api = profiles(fetch);
	const session = cookies.get(SESSION_COOKIE);
	try {
		if (session) {
			const own = await api.own(session).catch((failure: unknown) => {
				// Signed out or without an account: the profile as anyone sees it.
				if (failure instanceof Refused) return null;
				throw failure;
			});
			if (own && own.pseudonym.toLowerCase() === params.pseudonym.toLowerCase()) {
				return { profile: own, own: true };
			}
		}
		return { profile: await api.public(params.pseudonym), own: false };
	} catch (failure) {
		if (failure instanceof NotFound) error(404, 'No such author');
		if (failure instanceof Unavailable) error(503, 'The profile is unavailable');
		throw failure;
	}
}) satisfies PageServerLoad;
