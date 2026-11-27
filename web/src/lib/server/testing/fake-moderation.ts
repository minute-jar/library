// A stand-in for the library service's moderation API
// (`docs/specs/moderation-api.md`), over the sessions of the fake accounts
// API: it answers what a test sets and remembers the decisions it was given.

import type { FakeAccounts } from './fake-accounts.ts';
import type {
	AppealCase,
	Case,
	Figures,
	HeldReviewCase,
	History,
	ReportCase
} from '../moderation.ts';

export const rightsReport: ReportCase = {
	kind: 'report',
	report: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
	item: '0123456789abcdef0123456789abcdef',
	title: 'Irregular verbs, part 1',
	version: 2,
	reason: 'rights',
	details: 'Copied from English Grammar, Year 5.',
	effect: 'hidden',
	findings: null,
	notice: { name: 'Example Publishing', work: 'English Grammar, Year 5' },
	received_at: '2026-11-26T10:00:00Z',
	due_at: '2026-11-28T10:00:00Z'
};

export const authorAppeal: AppealCase = {
	kind: 'appeal',
	appeal: 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
	report: 'cccccccccccccccccccccccccccccccc',
	item: 'fedcba9876543210fedcba9876543210',
	title: 'Les fractions, CM1',
	reason: 'other',
	appellant: 'author',
	statement: 'Every question is about fractions.',
	decision: {
		outcome: 'upheld',
		reasons: 'Off topic.',
		moderator: 'Alice',
		decided_at: '2026-11-25T09:00:00Z'
	},
	received_at: '2026-11-26T12:00:00Z',
	due_at: '2026-11-28T12:00:00Z'
};

export const heldShare: HeldReviewCase = {
	kind: 'held_review',
	share: 'dddddddddddddddddddddddddddddddd',
	item: 'eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee',
	title: 'Capitals of Europe',
	pseudonym: 'Owl',
	findings: 'Question 2 may name a school.',
	type: 'bank',
	metadata: { language: 'en-GB', subject: 'geography', description: 'Forty capitals.' },
	content: { questions: [{ type: 'typed', prompt: 'Capital of Spain', answers: ['Madrid'] }] },
	received_at: '2026-11-26T14:00:00Z',
	due_at: '2026-11-28T14:00:00Z'
};

export const verbsHistory: History = {
	item: rightsReport.item,
	title: 'Irregular verbs, part 1',
	history: [
		{
			at: '2026-11-26T10:00:00Z',
			event: 'reported',
			report: rightsReport.report,
			reason: 'rights',
			details: 'Copied from English Grammar, Year 5.',
			effect: 'hidden',
			findings: null,
			notice: rightsReport.notice
		}
	],
	selection: null
};

export const figures2026: Figures = {
	year: 2026,
	reports: [
		{ reason: 'other', received: 2 },
		{ reason: 'rights', received: 1 }
	],
	decisions: [{ reason: 'rights', outcome: 'upheld', count: 1 }],
	appeals: [{ appellant: 'author', outcome: 'reversed', count: 1 }]
};

export type FakeModeration = {
	/** The moderation API's answers, as `fetch` gives them; null for another path. */
	fetch: (input: Request) => Promise<Response> | null;
	/** The method, path and body of each call, in order. */
	asked: { method: string; path: string; body: unknown }[];
	/** The moderators' handles, by Minute Jar account. */
	moderators: Map<string, string>;
	/** Whether every signed-in person is a moderator, as in the smoke tests. */
	anyoneModerates: boolean;
	/** Whether the sessions' sign-ins are by passkey and recent. */
	passkey: boolean;
	/** What waits in the queue. */
	cases: Case[];
	/** The histories, by item. */
	histories: Record<string, History>;
	/** The figures, by year. */
	figures: Record<number, Figures>;
};

const json = (body: unknown, status = 200) => Response.json(body, { status });
const refused = (status: number, error: string) => json({ error }, status);

export function fakeModeration(accounts: FakeAccounts): FakeModeration {
	async function answer(request: Request): Promise<Response> {
		const url = new URL(request.url);
		const body: unknown =
			request.method === 'GET' ? undefined : await request.json().catch(() => undefined);
		fake.asked.push({ method: request.method, path: url.pathname, body });
		const bearer = request.headers.get('authorization')?.replace(/^Bearer /, '') ?? '';
		const subject = accounts.sessions.get(bearer);
		if (!subject) return refused(401, 'signed_out');
		const handle = fake.moderators.get(subject) ?? (fake.anyoneModerates ? 'Moderator' : null);
		if (!handle) return refused(403, 'not_a_moderator');
		if (!fake.passkey) return refused(403, 'passkey_sign_in_required');

		const path = url.pathname;
		if (path === '/api/moderation/queue') return json({ cases: fake.cases });
		const marked = path.match(/^\/api\/moderation\/items\/([^/]+)\/selection$/);
		if (marked && (request.method === 'PUT' || request.method === 'DELETE')) {
			const history = fake.histories[marked[1]];
			if (!history) return refused(404, 'unknown_item');
			history.selection =
				request.method === 'PUT'
					? (history.selection ?? { moderator: handle, selected_at: '2026-11-27T10:00:00Z' })
					: null;
			return new Response(null, { status: 204 });
		}
		const item = path.match(/^\/api\/moderation\/items\/([^/]+)$/);
		if (item) {
			const history = fake.histories[item[1]];
			return history ? json(history) : refused(404, 'unknown_item');
		}
		const year = path.match(/^\/api\/moderation\/figures\/([^/]+)$/);
		if (year) {
			const figures = fake.figures[Number(year[1])];
			return figures
				? json(figures)
				: json({ year: Number(year[1]), reports: [], decisions: [], appeals: [] });
		}
		const held = path.match(/^\/api\/moderation\/shares\/([^/]+)\/decision$/);
		if (held && request.method === 'POST') {
			const { reasons } = (body ?? {}) as { reasons?: string };
			if (!reasons?.trim()) return refused(400, 'reasons_required');
			const index = fake.cases.findIndex(
				(waiting) => waiting.kind === 'held_review' && waiting.share === held[1]
			);
			if (index < 0) return refused(404, 'unknown_share');
			fake.cases.splice(index, 1);
			return new Response(null, { status: 204 });
		}
		const decided = path.match(/^\/api\/moderation\/(reports|appeals)\/([^/]+)\/decision$/);
		if (decided && request.method === 'POST') {
			const { outcome, reasons } = (body ?? {}) as { outcome?: string; reasons?: string };
			if (!reasons?.trim()) return refused(400, 'reasons_required');
			const [, kind, id] = decided;
			const index = fake.cases.findIndex((waiting) =>
				kind === 'reports'
					? waiting.kind === 'report' && waiting.report === id
					: waiting.kind === 'appeal' && waiting.appeal === id
			);
			if (index < 0) return refused(404, kind === 'reports' ? 'unknown_report' : 'unknown_appeal');
			const [done] = fake.cases.splice(index, 1);
			if (done.kind === 'held_review') return refused(404, 'unknown_report');
			if (done.kind === 'appeal' && done.decision.moderator === handle) {
				fake.cases.splice(index, 0, done);
				return refused(403, 'same_moderator');
			}
			fake.histories[done.item]?.history.push(
				done.kind === 'report'
					? {
							at: '2026-11-27T10:00:00Z',
							event: 'decided',
							report: done.report,
							reason: done.reason,
							outcome: outcome as 'upheld' | 'rejected',
							reasons,
							moderator: handle
						}
					: {
							at: '2026-11-27T10:00:00Z',
							event: 'appeal_decided',
							appeal: done.appeal,
							report: done.report,
							outcome: outcome as 'maintained' | 'reversed',
							reasons,
							moderator: handle
						}
			);
			return new Response(null, { status: 204 });
		}
		return refused(404, 'not_found');
	}

	const fake: FakeModeration = {
		asked: [],
		moderators: new Map(),
		anyoneModerates: false,
		passkey: true,
		cases: [structuredClone(rightsReport), structuredClone(authorAppeal)],
		histories: { [verbsHistory.item]: structuredClone(verbsHistory) },
		figures: { 2026: figures2026 },
		fetch(request) {
			const { pathname } = new URL(request.url);
			return pathname.startsWith('/api/moderation/') ? answer(request) : null;
		}
	};
	return fake;
}
