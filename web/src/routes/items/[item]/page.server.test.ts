import { describe, expect, it } from 'vitest';
import {
	fakeApi,
	moreVerbs,
	verbs,
	verbsPage,
	verbsRatings
} from '#lib/server/testing/fake-api.ts';
import { load } from './+page.server.ts';

type Event = Parameters<typeof load>[0];

async function visit(item: string, api = fakeApi()) {
	const event = { params: { item }, fetch: api.fetch };
	const data = await load(event as unknown as Event);
	return { data, api };
}

describe('the item page', () => {
	it('shows the item at its latest version with its ratings and reviews', async () => {
		const { data, api } = await visit(verbs.item);

		expect(data.item).toEqual(verbsPage);
		expect(data.ratings).toEqual(verbsRatings);
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
		const api = fakeApi();
		delete api.items[verbs.item];

		const { data } = await visit(moreVerbs.item, api);

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
		const api = fakeApi();
		api.failure = { status: 503, error: 'unavailable' };

		await expect(visit(verbs.item, api)).rejects.toMatchObject({ status: 503 });
	});
});
