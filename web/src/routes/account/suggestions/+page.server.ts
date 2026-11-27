// Suggestions to authors (story S18): the answers families suggested for
// the questions of the author's items, each counted by the families that
// suggested it and never telling who; the author ticks those to accept,
// and the new version goes to the review like any share.

import { error, fail, redirect } from '@sveltejs/kit';
import { localizeHref } from '#lib/paraglide/runtime.js';
import { Unavailable } from '#lib/server/catalogue.ts';
import { Refused, SESSION_COOKIE } from '#lib/server/accounts.ts';
import { type Accepted, type ItemSuggestions, suggestions } from '#lib/server/suggestions.ts';
import type { Actions, PageServerLoad } from './$types';

export const load = (async ({ fetch, cookies }) => {
	const session = cookies.get(SESSION_COOKIE);
	if (!session) redirect(303, localizeHref('/account'));
	let items: ItemSuggestions[];
	try {
		items = await suggestions(fetch).received(session);
	} catch (failure) {
		if (failure instanceof Refused) redirect(303, localizeHref('/account'));
		if (failure instanceof Unavailable) error(503, 'Suggestions are unavailable');
		throw failure;
	}
	return { items };
}) satisfies PageServerLoad;

/** The answers ticked, each sent as `<question>:<answer>`, by question. */
function ticked(form: FormData): Accepted[] {
	const accepted = new Map<number, string[]>();
	for (const value of form.getAll('accept')) {
		if (typeof value !== 'string') continue;
		const colon = value.indexOf(':');
		const question = Number(value.slice(0, colon));
		if (colon < 1 || !Number.isInteger(question)) continue;
		accepted.set(question, [...(accepted.get(question) ?? []), value.slice(colon + 1)]);
	}
	return [...accepted]
		.sort(([a], [b]) => a - b)
		.map(([question, answers]) => ({ question, answers }));
}

export const actions = {
	accept: async ({ fetch, cookies, request }) => {
		const session = cookies.get(SESSION_COOKIE);
		if (!session) return fail(401, { refused: 'signed_out', item: '' });
		const form = await request.formData();
		const item = form.get('item');
		if (typeof item !== 'string') return fail(400, { refused: 'bad_request', item: '' });
		const accepted = ticked(form);
		if (accepted.length === 0) return fail(400, { refused: 'answers_required', item });
		try {
			const shared = await suggestions(fetch).accept(session, item, accepted);
			return { item, shared };
		} catch (failure) {
			if (failure instanceof Refused) return fail(failure.status, { refused: failure.code, item });
			if (failure instanceof Unavailable) error(503, 'Suggestions are unavailable');
			throw failure;
		}
	}
} satisfies Actions;
