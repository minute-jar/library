import { describe, expect, it } from 'vitest';
import { SESSION_COOKIE } from '#lib/server/accounts.ts';
import { fakeAccounts, fakeCookies } from '#lib/server/testing/fake-accounts.ts';
import { fakeModeration, verbsHistory } from '#lib/server/testing/fake-moderation.ts';
import { load } from './+page.server.ts';

type Event = Parameters<typeof load>[0];

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
	return { load: load(event as unknown as Event), moderation };
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
});
