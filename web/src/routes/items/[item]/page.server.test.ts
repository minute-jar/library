import { describe, expect, it } from 'vitest';
import { SESSION_COOKIE } from '#lib/server/accounts.ts';
import {
	fakeApi,
	moreVerbs,
	verbs,
	verbsPage,
	verbsRatings
} from '#lib/server/testing/fake-api.ts';
import { fakeAccounts, fakeCookies } from '#lib/server/testing/fake-accounts.ts';
import { fakeComments } from '#lib/server/testing/fake-comments.ts';
import { actions, load } from './+page.server.ts';

type Event = Parameters<typeof load>[0];
type CommentEvent = Parameters<typeof actions.comment>[0];
type ReportEvent = Parameters<typeof actions.report>[0];

const lark = {
	pseudonym: 'Lark',
	minute_jar_account: 'separate' as const,
	opened_at: '2026-11-27T10:00:00Z'
};

function setUp() {
	const api = fakeApi();
	api.items[verbs.item] = structuredClone(verbsPage);
	const accounts = fakeAccounts();
	const comments = fakeComments(accounts, api);
	const fetch = (input: RequestInfo | URL, init?: RequestInit) => {
		const request = new Request(input, init);
		return comments.fetch(request) ?? accounts.fetch(request) ?? api.fetch(request);
	};
	return { api, accounts, comments, fetch };
}

function event(
	{ fetch }: ReturnType<typeof setUp>,
	item: string,
	session?: string,
	form?: Record<string, string>
) {
	const cookies = fakeCookies(session ? { [SESSION_COOKIE]: session } : {});
	const url = new URL(`/items/${item}`, 'https://library.example');
	const request = form
		? new Request(url, { method: 'POST', body: new URLSearchParams(form) })
		: new Request(url);
	return { params: { item }, url, fetch, cookies: cookies.cookies, request };
}

async function visit(item: string, set = setUp(), session?: string) {
	const data = await load(event(set, item, session) as unknown as Event);
	return { data, api: set.api };
}

const comment = (set: ReturnType<typeof setUp>, session: string | undefined, text: string) =>
	actions.comment(event(set, verbs.item, session, { text }) as unknown as CommentEvent);

describe('the item page', () => {
	it('shows the item at its latest version with its ratings and reviews', async () => {
		const { data, api } = await visit(verbs.item);

		expect(data.item).toEqual(verbsPage);
		expect(data.ratings).toEqual(verbsRatings);
		expect(data.signedIn).toBe(false);
		expect(data.original).toBeNull();
		expect(api.asked.sort()).toEqual([
			`/api/items/${verbs.item}`,
			`/api/items/${verbs.item}/ratings`
		]);
	});

	it('shows a derived work with its original, attributed and linked by title', async () => {
		const { data } = await visit(moreVerbs.item);

		expect(data.item).toEqual(moreVerbs);
		expect(data.original).toEqual({
			item: verbs.item,
			version: 1,
			pseudonym: 'Owl',
			title: 'Irregular verbs, part 1'
		});
	});

	it('keeps the attribution of an original the catalogue no longer shows', async () => {
		const set = setUp();
		delete set.api.items[verbs.item];

		const { data } = await visit(moreVerbs.item, set);

		expect(data.original).toEqual({
			item: verbs.item,
			version: 1,
			pseudonym: 'Owl',
			title: null
		});
	});

	it('answers 404 for an item the catalogue does not show', async () => {
		await expect(visit('fedcba9876543210fedcba9876543210')).rejects.toMatchObject({
			status: 404
		});
	});

	it('answers 503 when the API is unavailable', async () => {
		const set = setUp();
		set.api.failure = { status: 503, error: 'unavailable' };

		await expect(visit(verbs.item, set)).rejects.toMatchObject({ status: 503 });
	});
});

describe('comments on the item page', () => {
	it('publishes a comment the review lets through, under the pseudonym', async () => {
		const set = setUp();
		const session = set.accounts.signedIn('subject-lark', lark);

		const answer = await comment(set, session, '  Learnt in a week.  ');
		const { data } = await visit(verbs.item, set, session);

		expect(answer).toEqual({ commented: 'published', reasons: [] });
		expect(data.signedIn).toBe(true);
		expect(data.item.comments).toEqual([
			{
				comment: set.comments.written[0].comment,
				pseudonym: 'Lark',
				text: 'Learnt in a week.',
				published_at: '2026-11-27T10:00:00Z'
			}
		]);
	});

	it('tells the author a held comment waits for a moderator, and does not show it', async () => {
		const set = setUp();
		const session = set.accounts.signedIn('subject-lark', lark);
		set.comments.outcome = 'held';

		const answer = await comment(set, session, 'Tom loved it.');
		const { data } = await visit(verbs.item, set, session);

		expect(answer).toEqual({ commented: 'held', reasons: [] });
		expect(data.item.comments).toBeUndefined();
	});

	it('gives the review reasons of a refused comment, with the text to correct', async () => {
		const set = setUp();
		const session = set.accounts.signedIn('subject-lark', lark);
		set.comments.outcome = 'refused';
		set.comments.reasons = [
			{ question: null, reason: 'personal_data', explanation: 'It names a school.' }
		];

		const answer = await comment(set, session, 'We use it at Hillside Primary.');

		expect(answer).toEqual({
			commented: 'refused',
			reasons: set.comments.reasons,
			text: 'We use it at Hillside Primary.'
		});
	});

	it('asks to sign in, to open an account or to verify a card before commenting', async () => {
		const set = setUp();
		const signedOut = await comment(set, undefined, 'Nice.');
		const withoutAccount = await comment(set, set.accounts.signedIn('subject-new'), 'Nice.');
		set.comments.refusal = { status: 403, error: 'card_required' };
		const withoutCard = await comment(set, set.accounts.signedIn('subject-lark', lark), 'Nice.');

		expect(signedOut).toMatchObject({ status: 401, data: { refused: 'signed_out' } });
		expect(withoutAccount).toMatchObject({ status: 404, data: { refused: 'no_account' } });
		expect(withoutCard).toMatchObject({
			status: 403,
			data: { refused: 'card_required', text: 'Nice.' }
		});
		expect(set.comments.written).toEqual([]);
	});

	it('keeps the text of a comment the service finds empty or too long', async () => {
		const set = setUp();
		const session = set.accounts.signedIn('subject-lark', lark);

		const empty = await comment(set, session, '   ');

		expect(empty).toMatchObject({ status: 400, data: { refused: 'bad_comment', text: '   ' } });
	});

	it('sends the report of a published comment, anonymously', async () => {
		const set = setUp();
		await comment(set, set.accounts.signedIn('subject-lark', lark), 'Learnt in a week.');
		const reported = set.comments.written[0].comment;

		const answer = await actions.report(
			event(set, verbs.item, undefined, {
				comment: reported,
				reason: 'unsuitable_content',
				details: 'Rude.'
			}) as unknown as ReportEvent
		);

		expect(answer).toEqual({ reported });
		expect(
			set.comments.reports.map(({ comment, reason, details }) => ({ comment, reason, details }))
		).toEqual([
			{
				comment: reported,
				reason: 'unsuitable_content',
				details: 'Rude.'
			}
		]);
	});

	it('asks for a reason before sending a report', async () => {
		const set = setUp();

		const answer = await actions.report(
			event(set, verbs.item, undefined, {
				comment: '00000000000000000000000000000001',
				reason: ''
			}) as unknown as ReportEvent
		);

		expect(answer).toMatchObject({
			status: 400,
			data: { refused: 'reason_required', comment: '00000000000000000000000000000001' }
		});
		expect(set.comments.reports).toEqual([]);
	});
});
