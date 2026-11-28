// A stand-in for the comments routes of the library service, over the fake
// accounts and the fake catalogue API: a test sets the review's next
// outcome; a published comment joins the item page's comments.

import type { FakeAccounts } from './fake-accounts.ts';
import type { FakeApi } from './fake-api.ts';
import type { OwnComment } from '../comments.ts';
import type { RefusalReason } from '../shares.ts';

export type FakeComments = {
	/** The comments routes' answers; null for another path. */
	fetch: (input: Request) => Promise<Response> | null;
	/** What the review answers next. */
	outcome: 'published' | 'held' | 'refused';
	/** The reasons of a refusal. */
	reasons: RefusalReason[];
	/** A refusal the next comment gets before the review (`card_required`...), if any. */
	refusal: { status: number; error: string } | null;
	/** Every comment written, oldest first. */
	written: OwnComment[];
	/** The reports received, with their comment, reason and details. */
	reports: { report: string; comment: string; reason: string; details?: string }[];
};

const json = (body: unknown, status = 200) => Response.json(body, { status });
const refused = (status: number, error: string) => json({ error }, status);

export function fakeComments(accounts: FakeAccounts, api: FakeApi): FakeComments {
	let next = 0;
	const id = () => (++next).toString(16).padStart(32, '0');

	async function comment(request: Request, item: string): Promise<Response> {
		const bearer = request.headers.get('authorization')?.replace(/^Bearer /, '') ?? '';
		const subject = accounts.sessions.get(bearer);
		if (!subject) return refused(401, 'signed_out');
		const account = accounts.accounts.get(subject);
		if (!account) return refused(404, 'no_account');
		if (fake.refusal) return refused(fake.refusal.status, fake.refusal.error);
		const page = api.items[item];
		if (!page) return refused(404, 'unknown_item');
		const body = (await request.json()) as Record<string, unknown>;
		if (Object.keys(body).some((field) => field !== 'text') || typeof body.text !== 'string') {
			return refused(400, 'bad_request');
		}
		const text = body.text.trim();
		if (text.length < 1 || text.length > 1000) return refused(400, 'bad_comment');
		const written: OwnComment = {
			comment: id(),
			item,
			text,
			outcome: fake.outcome,
			reasons: fake.outcome === 'refused' ? fake.reasons : [],
			commented_at: '2026-11-27T10:00:00Z'
		};
		fake.written.push(written);
		if (written.outcome === 'published') {
			page.comments = [
				...(page.comments ?? []),
				{
					comment: written.comment,
					pseudonym: account.pseudonym,
					text,
					published_at: written.commented_at
				}
			];
		}
		const status = written.outcome === 'published' ? 201 : written.outcome === 'held' ? 202 : 200;
		return json(written, status);
	}

	async function report(request: Request, comment: string): Promise<Response> {
		const shown = Object.values(api.items).some((page) =>
			page.comments?.some((shown) => shown.comment === comment)
		);
		if (!shown) return refused(404, 'unknown_comment');
		const body = (await request.json()) as { reason: string; details?: string };
		const report = id();
		fake.reports.push({ report, comment, ...body });
		return json({ report }, 202);
	}

	const fake: FakeComments = {
		outcome: 'published',
		reasons: [],
		refusal: null,
		written: [],
		reports: [],
		fetch(request) {
			const { pathname } = new URL(request.url);
			if (request.method !== 'POST') return null;
			const commented = pathname.match(/^\/api\/items\/([^/]+)\/comments$/);
			if (commented) return comment(request, commented[1]);
			const reported = pathname.match(/^\/api\/comments\/([^/]+)\/reports$/);
			if (reported) return report(request, reported[1]);
			return null;
		}
	};
	return fake;
}
