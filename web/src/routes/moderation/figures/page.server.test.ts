import { describe, expect, it } from 'vitest';
import { SESSION_COOKIE } from '#lib/server/accounts.ts';
import { fakeAccounts, fakeCookies } from '#lib/server/testing/fake-accounts.ts';
import { fakeModeration, figures2026 } from '#lib/server/testing/fake-moderation.ts';
import { load } from './+page.server.ts';

type Event = Parameters<typeof load>[0];

function visit(path: string) {
	const accounts = fakeAccounts();
	const moderation = fakeModeration(accounts);
	moderation.moderators.set('subject-1', 'Bruno');
	const session = accounts.signedIn('subject-1');
	const fetch = (input: RequestInfo | URL, init?: RequestInit) =>
		moderation.fetch(new Request(input, init)) ??
		Promise.resolve(new Response(null, { status: 404 }));
	const event = {
		url: new URL(path, 'https://library.example'),
		fetch,
		cookies: fakeCookies({ [SESSION_COOKIE]: session }).cookies
	};
	return { load: load(event as unknown as Event), moderation };
}

describe('the transparency figures', () => {
	it("shows the year's counts by reason and outcome", async () => {
		const { load, moderation } = visit('/moderation/figures?year=2026');

		expect(await load).toEqual({ access: 'granted', year: 2026, figures: figures2026 });
		expect(moderation.asked.at(-1)?.path).toBe('/api/moderation/figures/2026');
	});

	it('shows the last full year when none is asked', async () => {
		const { load } = visit('/moderation/figures');

		expect((await load).year).toBe(new Date().getUTCFullYear() - 1);
	});

	it('answers 400 for a year that is not one', async () => {
		await expect(visit('/moderation/figures?year=last').load).rejects.toMatchObject({
			status: 400
		});
	});
});
