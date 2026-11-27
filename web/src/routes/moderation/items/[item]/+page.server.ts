// An item's history in moderation: its reports, the decisions on them and
// their appeals, oldest first, so that a moderator decides consistently.

import { error } from '@sveltejs/kit';
import { Refused, SESSION_COOKIE } from '#lib/server/accounts.ts';
import { type History, access, moderation } from '#lib/server/moderation.ts';
import type { PageServerLoad } from './$types';

export const load = (async ({ params, fetch, cookies }) => {
	const session = cookies.get(SESSION_COOKIE);
	if (!session) return { access: 'signed_out', history: null as History | null };
	try {
		const history = await moderation(fetch).history(session, params.item);
		return { access: 'granted', history };
	} catch (failure) {
		const refused = access(failure);
		if (refused) return { access: refused, history: null as History | null };
		if (failure instanceof Refused && failure.code === 'unknown_item') error(404, 'Unknown item');
		error(503, 'Moderation is unavailable');
	}
}) satisfies PageServerLoad;
