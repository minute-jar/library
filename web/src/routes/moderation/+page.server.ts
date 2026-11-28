// The moderation queue: the reports, appeals, held shares and comments
// waiting for a moderator,
// oldest first, each decided here with reasons sent to the author and the
// reporter. Only a moderator signed in with a recent passkey gets in.

import { error, fail } from '@sveltejs/kit';
import { Refused, SESSION_COOKIE } from '#lib/server/accounts.ts';
import { Unavailable } from '#lib/server/catalogue.ts';
import {
	type AppealOutcome,
	type Case,
	type HeldOutcome,
	type Outcome,
	type Reason,
	access,
	moderation
} from '#lib/server/moderation.ts';
import type { Actions, PageServerLoad } from './$types';

export const load = (async ({ fetch, cookies }) => {
	const session = cookies.get(SESSION_COOKIE);
	if (!session) return { access: 'signed_out', cases: [] as Case[] };
	try {
		return { access: 'granted', cases: await moderation(fetch).queue(session) };
	} catch (failure) {
		const refused = access(failure);
		if (refused) return { access: refused, cases: [] as Case[] };
		error(503, 'Moderation is unavailable');
	}
}) satisfies PageServerLoad;

/** A text field of the form, empty when missing. */
function text(form: FormData, name: string): string {
	const value = form.get(name);
	return typeof value === 'string' ? value : '';
}

const REASONS: Reason[] = [
	'personal_data',
	'sexual_content',
	'illegal_content',
	'rights',
	'unsuitable_content',
	'wrong_answer',
	'other'
];

/** Asks the service to decide, then answers what the page shows. */
async function deciding(
	request: Request,
	cookies: { get(name: string): string | undefined },
	field: 'report' | 'appeal' | 'share' | 'comment' | 'comment_report',
	outcomes: string[],
	decide: (
		session: string,
		id: string,
		outcome: string,
		reasons: string,
		form: FormData
	) => Promise<void>
) {
	const form = await request.formData();
	const id = text(form, field);
	const outcome = text(form, 'outcome');
	const reasons = text(form, 'reasons');
	const values = { case: id, outcome, reasons };
	const session = cookies.get(SESSION_COOKIE);
	if (!session) return fail(401, { refused: 'signed_out', ...values });
	if (!outcomes.includes(outcome)) return fail(400, { refused: 'outcome_required', ...values });
	if (
		(field === 'share' || field === 'comment') &&
		!REASONS.includes(text(form, 'reason') as Reason)
	) {
		return fail(400, { refused: 'reason_required', ...values });
	}
	try {
		await decide(session, id, outcome, reasons, form);
		return { decided: id };
	} catch (failure) {
		if (failure instanceof Refused)
			return fail(failure.status, { refused: failure.code, ...values });
		if (failure instanceof Unavailable) error(503, 'Moderation is unavailable');
		throw failure;
	}
}

export const actions = {
	decideReport: ({ request, fetch, cookies }) =>
		deciding(request, cookies, 'report', ['upheld', 'rejected'], (session, id, outcome, reasons) =>
			moderation(fetch).decideReport(session, id, outcome as Outcome, reasons)
		),
	decideAppeal: ({ request, fetch, cookies }) =>
		deciding(
			request,
			cookies,
			'appeal',
			['maintained', 'reversed'],
			(session, id, outcome, reasons) =>
				moderation(fetch).decideAppeal(session, id, outcome as AppealOutcome, reasons)
		),
	decideShare: ({ request, fetch, cookies }) =>
		deciding(
			request,
			cookies,
			'share',
			['published', 'refused'],
			(session, id, outcome, reasons, form) =>
				moderation(fetch).decideShare(
					session,
					id,
					outcome as HeldOutcome,
					text(form, 'reason') as Reason,
					reasons
				)
		),
	decideComment: ({ request, fetch, cookies }) =>
		deciding(
			request,
			cookies,
			'comment',
			['published', 'refused'],
			(session, id, outcome, reasons, form) =>
				moderation(fetch).decideComment(
					session,
					id,
					outcome as HeldOutcome,
					text(form, 'reason') as Reason,
					reasons
				)
		),
	decideCommentReport: ({ request, fetch, cookies }) =>
		deciding(
			request,
			cookies,
			'comment_report',
			['upheld', 'rejected'],
			(session, id, outcome, reasons) =>
				moderation(fetch).decideCommentReport(session, id, outcome as Outcome, reasons)
		)
} satisfies Actions;
