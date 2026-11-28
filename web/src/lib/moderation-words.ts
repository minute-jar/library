// The words the administration pages show for the moderation API's codes.

import { getLocale } from '#lib/paraglide/runtime.js';
import { m } from '#lib/paraglide/messages.js';
import type { Case } from '#lib/server/moderation.ts';

const reasons: Record<string, () => string> = {
	personal_data: m.reason_personal_data,
	sexual_content: m.reason_sexual_content,
	illegal_content: m.reason_illegal_content,
	rights: m.reason_rights,
	unsuitable_content: m.reason_unsuitable_content,
	wrong_answer: m.reason_wrong_answer,
	other: m.reason_other
};

const outcomes: Record<string, () => string> = {
	upheld: m.word_upheld,
	rejected: m.word_rejected,
	maintained: m.word_maintained,
	reversed: m.word_reversed,
	published: m.word_published,
	refused: m.word_refused,
	waiting: m.word_waiting
};

const cases: Record<string, () => string> = {
	report: m.figures_case_report,
	comment_report: m.figures_case_comment_report,
	held_review: m.figures_case_held_review,
	held_comment: m.figures_case_held_comment
};

const appellants: Record<string, () => string> = {
	author: m.word_author,
	reporter: m.word_reporter
};

const refusals: Record<string, () => string> = {
	reasons_required: m.refused_reasons_required,
	outcome_required: m.refused_outcome_required,
	same_moderator: m.refused_same_moderator,
	already_decided: m.refused_already_decided,
	reason_required: m.refused_reason_required,
	unknown_share: m.refused_other,
	unknown_comment: m.refused_other
};

export const reasonWord = (code: string) => (reasons[code] ?? m.reason_other)();
export const outcomeWord = (code: string) => (outcomes[code] ?? (() => code))();
export const caseWord = (code: string) => (cases[code] ?? (() => code))();
export const appellantWord = (code: string) => (appellants[code] ?? (() => code))();
export const refusalWord = (code: string) => (refusals[code] ?? m.refused_other)();

export const day = (date: string) =>
	new Intl.DateTimeFormat(getLocale(), { dateStyle: 'long', timeZone: 'UTC' }).format(
		new Date(date)
	);
export const moment = (date: string) =>
	new Intl.DateTimeFormat(getLocale(), {
		dateStyle: 'medium',
		timeStyle: 'short',
		timeZone: 'UTC'
	}).format(new Date(date));

/** A case's identifier, whatever its kind. */
export function caseId(waiting: Case): string {
	if (waiting.kind === 'report') return waiting.report;
	if (waiting.kind === 'appeal') return waiting.appeal;
	if (waiting.kind === 'held_comment') return waiting.comment;
	if (waiting.kind === 'comment_report') return waiting.report;
	return waiting.share;
}
