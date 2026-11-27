// The words the administration pages show for the moderation API's codes.

import { getLocale } from '#lib/paraglide/runtime.js';
import { m } from '#lib/paraglide/messages.js';

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
	waiting: m.word_waiting
};

const appellants: Record<string, () => string> = {
	author: m.word_author,
	reporter: m.word_reporter
};

const refusals: Record<string, () => string> = {
	reasons_required: m.refused_reasons_required,
	outcome_required: m.refused_outcome_required,
	same_moderator: m.refused_same_moderator,
	already_decided: m.refused_already_decided
};

export const reasonWord = (code: string) => (reasons[code] ?? m.reason_other)();
export const outcomeWord = (code: string) => (outcomes[code] ?? (() => code))();
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
