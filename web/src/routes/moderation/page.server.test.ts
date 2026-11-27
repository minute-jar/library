import { describe, expect, it } from 'vitest';
import { SESSION_COOKIE } from '#lib/server/accounts.ts';
import { fakeAccounts, fakeCookies } from '#lib/server/testing/fake-accounts.ts';
import {
	authorAppeal,
	fakeModeration,
	heldShare,
	rightsReport
} from '#lib/server/testing/fake-moderation.ts';
import { actions, load } from './+page.server.ts';

type LoadEvent = Parameters<typeof load>[0];
type ActionEvent = Parameters<typeof actions.decideReport>[0];

/** A moderation API whose Minute Jar account `subject-1` is the moderator Bruno. */
function api() {
	const accounts = fakeAccounts();
	const moderation = fakeModeration(accounts);
	moderation.moderators.set('subject-1', 'Bruno');
	return { accounts, moderation };
}

function event(
	{ accounts, moderation }: ReturnType<typeof api>,
	session?: string,
	form?: Record<string, string>
) {
	const fetch = (input: RequestInfo | URL, init?: RequestInit) => {
		const request = new Request(input, init);
		return (
			moderation.fetch(request) ??
			accounts.fetch(request) ??
			Promise.resolve(new Response(null, { status: 404 }))
		);
	};
	const cookies = fakeCookies(session ? { [SESSION_COOKIE]: session } : {});
	const url = new URL('/moderation', 'https://library.example');
	const request = new Request(url, {
		method: form ? 'POST' : 'GET',
		body: form ? new URLSearchParams(form) : undefined
	});
	return { url, fetch, cookies: cookies.cookies, request };
}

describe('the moderation queue', () => {
	it('shows a moderator the reports and appeals waiting, oldest first', async () => {
		const fakes = api();
		const session = fakes.accounts.signedIn('subject-1');

		const data = await load(event(fakes, session) as unknown as LoadEvent);

		expect(data).toEqual({ access: 'granted', cases: [rightsReport, authorAppeal] });
		expect(fakes.moderation.asked).toEqual([
			{ method: 'GET', path: '/api/moderation/queue', body: undefined }
		]);
	});

	it('asks to sign in when signed out, without asking the service', async () => {
		const fakes = api();

		const data = await load(event(fakes) as unknown as LoadEvent);

		expect(data).toEqual({ access: 'signed_out', cases: [] });
		expect(fakes.moderation.asked).toEqual([]);
	});

	it('refuses someone who is not a moderator', async () => {
		const fakes = api();
		const session = fakes.accounts.signedIn('subject-2');

		const data = await load(event(fakes, session) as unknown as LoadEvent);

		expect(data).toEqual({ access: 'not_a_moderator', cases: [] });
	});

	it('asks a moderator to sign in again with a passkey', async () => {
		const fakes = api();
		fakes.moderation.passkey = false;
		const session = fakes.accounts.signedIn('subject-1');

		const data = await load(event(fakes, session) as unknown as LoadEvent);

		expect(data).toEqual({ access: 'passkey_required', cases: [] });
	});

	it('answers 503 when the service is unavailable', async () => {
		const fakes = api();
		const session = fakes.accounts.signedIn('subject-1');
		const broken = event(fakes, session);
		broken.fetch = () => Promise.resolve(Response.json({ error: 'unavailable' }, { status: 503 }));

		await expect(load(broken as unknown as LoadEvent)).rejects.toMatchObject({ status: 503 });
	});

	it('decides a report with its reasons', async () => {
		const fakes = api();
		const session = fakes.accounts.signedIn('subject-1');

		const answer = await actions.decideReport(
			event(fakes, session, {
				report: rightsReport.report,
				outcome: 'upheld',
				reasons: 'The questions copy the workbook.'
			}) as unknown as ActionEvent
		);

		expect(answer).toEqual({ decided: rightsReport.report });
		expect(fakes.moderation.asked.at(-1)).toEqual({
			method: 'POST',
			path: `/api/moderation/reports/${rightsReport.report}/decision`,
			body: { outcome: 'upheld', reasons: 'The questions copy the workbook.' }
		});
		expect(fakes.moderation.cases).toEqual([authorAppeal]);
	});

	it('refuses an outcome it does not know without asking the service', async () => {
		const fakes = api();
		const session = fakes.accounts.signedIn('subject-1');

		const answer = await actions.decideReport(
			event(fakes, session, {
				report: rightsReport.report,
				outcome: 'maybe',
				reasons: 'Unclear.'
			}) as unknown as ActionEvent
		);

		expect(answer).toMatchObject({ status: 400, data: { refused: 'outcome_required' } });
		expect(fakes.moderation.asked).toEqual([]);
	});

	it('passes on the refusal of a decision without reasons', async () => {
		const fakes = api();
		const session = fakes.accounts.signedIn('subject-1');

		const answer = await actions.decideReport(
			event(fakes, session, {
				report: rightsReport.report,
				outcome: 'rejected',
				reasons: ' '
			}) as unknown as ActionEvent
		);

		expect(answer).toMatchObject({
			status: 400,
			data: { refused: 'reasons_required', case: rightsReport.report }
		});
	});

	it('decides an appeal, and passes on the refusal of the moderator who decided', async () => {
		const fakes = api();
		const bruno = fakes.accounts.signedIn('subject-1');
		fakes.moderation.moderators.set('subject-3', 'Alice');
		const alice = fakes.accounts.signedIn('subject-3');
		const form = {
			appeal: authorAppeal.appeal,
			outcome: 'reversed',
			reasons: 'The questions are on topic.'
		};

		const refused = await actions.decideAppeal(event(fakes, alice, form) as unknown as ActionEvent);
		const answer = await actions.decideAppeal(event(fakes, bruno, form) as unknown as ActionEvent);

		expect(refused).toMatchObject({ status: 403, data: { refused: 'same_moderator' } });
		expect(answer).toEqual({ decided: authorAppeal.appeal });
		expect(fakes.moderation.asked.at(-1)).toEqual({
			method: 'POST',
			path: `/api/moderation/appeals/${authorAppeal.appeal}/decision`,
			body: { outcome: 'reversed', reasons: 'The questions are on topic.' }
		});
	});

	it('shows a share the review held, with its findings and its content', async () => {
		const fakes = api();
		fakes.moderation.cases.push(structuredClone(heldShare));
		const session = fakes.accounts.signedIn('subject-1');

		const data = await load(event(fakes, session) as unknown as LoadEvent);

		expect(data.cases.at(-1)).toEqual(heldShare);
	});

	it('publishes or refuses a held share, naming the rule its findings were about', async () => {
		const fakes = api();
		fakes.moderation.cases.push(structuredClone(heldShare));
		const session = fakes.accounts.signedIn('subject-1');
		const form = {
			share: heldShare.share,
			outcome: 'refused',
			reason: 'personal_data',
			reasons: 'Question 2 names a real school.'
		};

		const withoutReason = await actions.decideShare(
			event(fakes, session, { ...form, reason: '' }) as unknown as ActionEvent
		);
		const answer = await actions.decideShare(event(fakes, session, form) as unknown as ActionEvent);

		expect(withoutReason).toMatchObject({ status: 400, data: { refused: 'reason_required' } });
		expect(answer).toEqual({ decided: heldShare.share });
		expect(fakes.moderation.asked.at(-1)).toEqual({
			method: 'POST',
			path: `/api/moderation/shares/${heldShare.share}/decision`,
			body: {
				outcome: 'refused',
				reason: 'personal_data',
				reasons: 'Question 2 names a real school.'
			}
		});
	});

	it('refuses to decide when signed out', async () => {
		const answer = await actions.decideReport(
			event(api(), undefined, {
				report: rightsReport.report,
				outcome: 'upheld',
				reasons: 'Reasons.'
			}) as unknown as ActionEvent
		);

		expect(answer).toMatchObject({ status: 401, data: { refused: 'signed_out' } });
	});
});
