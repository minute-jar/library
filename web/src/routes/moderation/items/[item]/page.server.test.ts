import { describe, expect, it } from 'vitest';
import { SESSION_COOKIE } from '#lib/server/accounts.ts';
import { fakeAccounts, fakeCookies } from '#lib/server/testing/fake-accounts.ts';
import { fakeModeration, verbsHistory } from '#lib/server/testing/fake-moderation.ts';
import type { History } from '#lib/server/moderation.ts';
import { actions, load } from './+page.server.ts';

type Event = Parameters<typeof load>[0];
type ActionEvent = Parameters<(typeof actions)['select']>[0];

function visit(item: string, moderator = true) {
	const accounts = fakeAccounts();
	const moderation = fakeModeration(accounts);
	if (moderator) moderation.moderators.set('subject-1', 'Bruno');
	const session = accounts.signedIn('subject-1');
	const fetch = (input: RequestInfo | URL, init?: RequestInit) =>
		moderation.fetch(new Request(input, init)) ??
		Promise.resolve(new Response(null, { status: 404 }));
	const event = {
		params: { item },
		fetch,
		cookies: fakeCookies({ [SESSION_COOKIE]: session }).cookies
	};
	return { load: load(event as unknown as Event), moderation, event };
}

/** Sends the Selection form of the item page. */
async function mark(item: string, action: 'select' | 'unselect', moderator = true) {
	const visited = visit(item, moderator);
	// The page was shown first; only the form's call counts.
	await visited.load;
	visited.moderation.asked.length = 0;
	const request = new Request(`https://library.example/moderation/items/${item}`, {
		method: 'POST',
		body: new FormData()
	});
	const event = { ...visited.event, request } as unknown as ActionEvent;
	return { answer: actions[action](event), moderation: visited.moderation };
}

describe("an item's history in moderation", () => {
	it('shows its reports, decisions and appeals, oldest first', async () => {
		const { load, moderation } = visit(verbsHistory.item);

		expect(await load).toEqual({ access: 'granted', history: verbsHistory });
		expect(moderation.asked).toEqual([
			{ method: 'GET', path: `/api/moderation/items/${verbsHistory.item}`, body: undefined }
		]);
	});

	it('answers 404 for an item the service does not know', async () => {
		await expect(visit('00000000000000000000000000000000').load).rejects.toMatchObject({
			status: 404
		});
	});

	it('refuses someone who is not a moderator', async () => {
		expect(await visit(verbsHistory.item, false).load).toEqual({
			access: 'not_a_moderator',
			history: null
		});
	});

	it('says who marked the item Selection, and when', async () => {
		const { load } = visit(verbsHistory.item);

		expect(((await load) as { history: History }).history.selection).toBeNull();
	});

	it('marks the item Selection', async () => {
		const { answer, moderation } = await mark(verbsHistory.item, 'select');

		expect(await answer).toEqual({ selection: 'selected' });
		expect(moderation.asked).toEqual([
			{
				method: 'PUT',
				path: `/api/moderation/items/${verbsHistory.item}/selection`,
				body: undefined
			}
		]);
		expect(moderation.histories[verbsHistory.item].selection).toEqual({
			moderator: 'Bruno',
			selected_at: '2026-11-27T10:00:00Z'
		});
	});

	it('removes the Selection mark', async () => {
		const { answer, moderation } = await mark(verbsHistory.item, 'unselect');

		expect(await answer).toEqual({ selection: 'unselected' });
		expect(moderation.asked[0]).toMatchObject({
			method: 'DELETE',
			path: `/api/moderation/items/${verbsHistory.item}/selection`
		});
	});

	it('refuses the mark to someone who is not a moderator', async () => {
		const { answer } = await mark(verbsHistory.item, 'select', false);

		expect(await answer).toMatchObject({ status: 403, data: { refused: 'not_a_moderator' } });
	});
});
