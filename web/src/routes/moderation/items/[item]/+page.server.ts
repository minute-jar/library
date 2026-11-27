// An item's history in moderation: its reports, the decisions on them and
// their appeals, oldest first, so that a moderator decides consistently;
// and its Selection mark, which a moderator sets or removes here.

import { error, fail } from '@sveltejs/kit';
import { Refused, SESSION_COOKIE } from '#lib/server/accounts.ts';
import { Unavailable } from '#lib/server/catalogue.ts';
import { type History, access, moderation } from '#lib/server/moderation.ts';
import type { Actions, PageServerLoad } from './$types';

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

/** Sets or removes the Selection mark, then answers what the page shows. */
async function marking(
	{ params, fetch, cookies }: Parameters<Actions[string]>[0],
	selected: boolean
) {
	const session = cookies.get(SESSION_COOKIE);
	if (!session) return fail(401, { refused: 'signed_out' });
	try {
		await moderation(fetch).mark(session, params.item ?? '', selected);
		return { selection: selected ? 'selected' : 'unselected' };
	} catch (failure) {
		if (failure instanceof Refused) return fail(failure.status, { refused: failure.code });
		if (failure instanceof Unavailable) error(503, 'Moderation is unavailable');
		throw failure;
	}
}

export const actions = {
	select: (event) => marking(event, true),
	unselect: (event) => marking(event, false)
} satisfies Actions;
