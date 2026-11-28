// The yearly transparency report's figures, from the decisions log: counts
// by reason and outcome, without personal data.

import { error } from '@sveltejs/kit';
import { SESSION_COOKIE } from '#lib/server/accounts.ts';
import { type Figures, access, moderation } from '#lib/server/moderation.ts';
import type { PageServerLoad } from './$types';

export const load = (async ({ url, fetch, cookies }) => {
	const asked = url.searchParams.get('year');
	// The report covers a full year: the last one, unless another is asked.
	const year = asked === null ? new Date().getUTCFullYear() - 1 : Number(asked);
	if (!Number.isInteger(year) || year < 2000 || year > 9999) error(400, 'Not a year');
	const session = cookies.get(SESSION_COOKIE);
	if (!session) return { access: 'signed_out', year, figures: null as Figures | null };
	try {
		const figures = await moderation(fetch).figures(session, year);
		return { access: 'granted', year, figures };
	} catch (failure) {
		const refused = access(failure);
		if (refused) return { access: refused, year, figures: null as Figures | null };
		error(503, 'Moderation is unavailable');
	}
}) satisfies PageServerLoad;
