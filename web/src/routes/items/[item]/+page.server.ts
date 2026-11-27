// A shared item's page: the item, its ratings and its published comments.
// A library account comments under its pseudonym, every comment reviewed
// before it shows (stories S28, S29); anyone reports a published comment.
// A comment is addressed to the item, never to an account.

import { error, fail } from '@sveltejs/kit';
import { NotFound, items } from '#lib/server/items.ts';
import { Refused, SESSION_COOKIE } from '#lib/server/accounts.ts';
import { Unavailable } from '#lib/server/catalogue.ts';
import { REPORT_REASONS, comments } from '#lib/server/comments.ts';
import type { Actions, PageServerLoad } from './$types';

export const load = (async ({ params, fetch, cookies }) => {
	const api = items(fetch);
	try {
		const [item, ratings] = await Promise.all([api.page(params.item), api.ratings(params.item)]);
		return { item, ratings, signedIn: Boolean(cookies.get(SESSION_COOKIE)) };
	} catch (failure) {
		if (failure instanceof NotFound) error(404, 'No such shared item');
		error(503, 'The catalogue is unavailable');
	}
}) satisfies PageServerLoad;

/** A text field of the form, empty when missing. */
function text(form: FormData, name: string): string {
	const value = form.get(name);
	return typeof value === 'string' ? value : '';
}

export const actions = {
	comment: async ({ params, fetch, cookies, request }) => {
		const written = text(await request.formData(), 'text');
		const session = cookies.get(SESSION_COOKIE);
		if (!session) return fail(401, { refused: 'signed_out', text: written });
		try {
			const commented = await comments(fetch).comment(session, params.item, written);
			if (commented.outcome === 'refused') {
				return { commented: commented.outcome, reasons: commented.reasons, text: written };
			}
			return { commented: commented.outcome, reasons: commented.reasons };
		} catch (failure) {
			if (failure instanceof Refused)
				return fail(failure.status, { refused: failure.code, text: written });
			if (failure instanceof Unavailable) error(503, 'Comments are unavailable');
			throw failure;
		}
	},
	report: async ({ fetch, request }) => {
		const form = await request.formData();
		const comment = text(form, 'comment');
		const reason = text(form, 'reason');
		const details = text(form, 'details');
		if (!(REPORT_REASONS as readonly string[]).includes(reason)) {
			return fail(400, { refused: 'reason_required', comment, details });
		}
		try {
			await comments(fetch).report(comment, reason, details);
			return { reported: comment };
		} catch (failure) {
			if (failure instanceof Refused)
				return fail(failure.status, { refused: failure.code, comment, details });
			if (failure instanceof Unavailable) error(503, 'Comments are unavailable');
			throw failure;
		}
	}
} satisfies Actions;
