// The library service's moderation API (`docs/specs/moderation-api.md`), as
// the website's administration pages read it. Every call carries the
// session of a moderator signed in with a recent passkey.

import { LIBRARY_API } from '$app/env/private';
import { Refused } from './accounts.ts';
import { Unavailable } from './catalogue.ts';

export type Reason =
	| 'personal_data'
	| 'sexual_content'
	| 'illegal_content'
	| 'rights'
	| 'unsuitable_content'
	| 'wrong_answer'
	| 'other';
export type Outcome = 'upheld' | 'rejected';
export type AppealOutcome = 'maintained' | 'reversed';
export type Appellant = 'author' | 'reporter';
export type Effect = 'none' | 'switched_off' | 'hidden';

/** A rights holder's formal notice. */
export type Notice = { name: string; work: string };

/** A report waiting for a moderator. */
export type ReportCase = {
	kind: 'report';
	report: string;
	item: string;
	title: string | null;
	version: number;
	reason: Reason;
	details: string | null;
	effect: Effect;
	findings: string | null;
	notice: Notice | null;
	received_at: string;
	due_at: string;
};

/** An appeal waiting for another moderator than the one who decided. */
export type AppealCase = {
	kind: 'appeal';
	appeal: string;
	report: string;
	item: string;
	title: string | null;
	reason: Reason;
	appellant: Appellant;
	statement: string;
	decision: { outcome: Outcome; reasons: string; moderator: string; decided_at: string };
	received_at: string;
	due_at: string;
};

/** A share the review held for a moderator, who publishes or refuses it. */
export type HeldReviewCase = {
	kind: 'held_review';
	share: string;
	item: string;
	title: string;
	/** The author's pseudonym, under which it would be published. */
	pseudonym: string;
	findings: string;
	type: 'bank' | 'path';
	metadata: { language: string; subject: string; description: string };
	/** The bank or learning path, in the bank format. */
	content: unknown;
	received_at: string;
	due_at: string;
};

export type HeldOutcome = 'published' | 'refused';

/** A comment the review held for a moderator, who publishes or refuses it. */
export type HeldCommentCase = {
	kind: 'held_comment';
	comment: string;
	item: string;
	title: string | null;
	/** The author's pseudonym, under which it would be published. */
	pseudonym: string;
	text: string;
	findings: string;
	received_at: string;
	due_at: string;
};

/** A report of a published comment; upheld, the comment is removed. */
export type CommentReportCase = {
	kind: 'comment_report';
	report: string;
	comment: string;
	item: string;
	title: string | null;
	pseudonym: string;
	text: string;
	reason: Reason;
	details: string | null;
	/** Whether the comment is hidden until the decision. */
	hidden: boolean;
	received_at: string;
	due_at: string;
};

/** What waits in the one queue. */
export type Case = ReportCase | AppealCase | HeldReviewCase | HeldCommentCase | CommentReportCase;

export type HistoryEntry =
	| {
			at: string;
			event: 'reported';
			report: string;
			reason: Reason;
			details: string | null;
			effect: Effect;
			findings: string | null;
			notice: Notice | null;
	  }
	| { at: string; event: 'corrected'; report: string }
	| {
			at: string;
			event: 'decided';
			report: string;
			reason: Reason;
			outcome: Outcome;
			reasons: string;
			moderator: string;
	  }
	| {
			at: string;
			event: 'appealed';
			appeal: string;
			report: string;
			appellant: Appellant;
			statement: string;
	  }
	| {
			at: string;
			event: 'appeal_decided';
			appeal: string;
			report: string;
			outcome: AppealOutcome;
			reasons: string;
			moderator: string;
	  };

/** Who marked an item Selection, and when. */
export type Selection = { moderator: string; selected_at: string };

export type History = {
	item: string;
	title: string;
	history: HistoryEntry[];
	/** Null when the item is not marked Selection. */
	selection: Selection | null;
};

/** The yearly transparency report's figures. */
export type Figures = {
	year: number;
	reports: { reason: Reason; received: number }[];
	decisions: { reason: Reason; outcome: Outcome; count: number }[];
	appeals: { appellant: Appellant; outcome: AppealOutcome | 'waiting'; count: number }[];
};

/** Who may moderate: a moderator, or why not. */
export type Access = 'granted' | 'signed_out' | 'not_a_moderator' | 'passkey_required';

/** The access a refusal of the moderation API means, if it means one. */
export function access(failure: unknown): Access | null {
	if (!(failure instanceof Refused)) return null;
	if (failure.code === 'signed_out') return 'signed_out';
	if (failure.code === 'not_a_moderator') return 'not_a_moderator';
	if (failure.code === 'passkey_sign_in_required') return 'passkey_required';
	return null;
}

export function moderation(fetch: typeof globalThis.fetch, root: string = LIBRARY_API) {
	async function call<T>(method: string, path: string, session: string, body?: unknown) {
		const headers: Record<string, string> = { authorization: `Bearer ${session}` };
		if (body !== undefined) headers['content-type'] = 'application/json';
		let response: Response;
		try {
			response = await fetch(new URL(path, root), {
				method,
				headers,
				body: body === undefined ? undefined : JSON.stringify(body)
			});
		} catch (cause) {
			throw new Unavailable('the moderation API is unreachable', { cause });
		}
		if (response.status === 204) return undefined as T;
		if (response.status >= 500)
			throw new Unavailable(`the moderation API answered ${response.status}`);
		const answer = (await response.json()) as T & { error?: string };
		if (!response.ok) throw new Refused(response.status, answer.error ?? 'bad_request');
		return answer;
	}

	return {
		async queue(session: string) {
			return (await call<{ cases: Case[] }>('GET', '/api/moderation/queue', session)).cases;
		},
		history(session: string, item: string) {
			return call<History>('GET', `/api/moderation/items/${encodeURIComponent(item)}`, session);
		},
		decideReport(session: string, report: string, outcome: Outcome, reasons: string) {
			return call<void>(
				'POST',
				`/api/moderation/reports/${encodeURIComponent(report)}/decision`,
				session,
				{ outcome, reasons }
			);
		},
		decideAppeal(session: string, appeal: string, outcome: AppealOutcome, reasons: string) {
			return call<void>(
				'POST',
				`/api/moderation/appeals/${encodeURIComponent(appeal)}/decision`,
				session,
				{ outcome, reasons }
			);
		},
		/** Publishes or refuses a held share, naming the rule its findings were about. */
		decideShare(
			session: string,
			share: string,
			outcome: HeldOutcome,
			reason: Reason,
			reasons: string
		) {
			return call<void>(
				'POST',
				`/api/moderation/shares/${encodeURIComponent(share)}/decision`,
				session,
				{ outcome, reason, reasons }
			);
		},
		/** Publishes or refuses a held comment, naming the rule its findings were about. */
		decideComment(
			session: string,
			comment: string,
			outcome: HeldOutcome,
			reason: Reason,
			reasons: string
		) {
			return call<void>(
				'POST',
				`/api/moderation/comments/${encodeURIComponent(comment)}/decision`,
				session,
				{ outcome, reason, reasons }
			);
		},
		/** Upholds the report of a comment, which removes it, or rejects it. */
		decideCommentReport(session: string, report: string, outcome: Outcome, reasons: string) {
			return call<void>(
				'POST',
				`/api/moderation/comment-reports/${encodeURIComponent(report)}/decision`,
				session,
				{ outcome, reasons }
			);
		},
		/** Marks the item Selection, or removes the mark; the index is signed again. */
		mark(session: string, item: string, selected: boolean) {
			return call<void>(
				selected ? 'PUT' : 'DELETE',
				`/api/moderation/items/${encodeURIComponent(item)}/selection`,
				session
			);
		},
		figures(session: string, year: number) {
			return call<Figures>('GET', `/api/moderation/figures/${year}`, session);
		}
	};
}
