import { describe, expect, it } from 'vitest';
import { fakeApi, fractions, offered, verbs } from '#lib/server/testing/fake-api.ts';
import { load } from './+page.server.ts';

type Event = Parameters<typeof load>[0];

async function visit(path: string, api = fakeApi()) {
	const event = { url: new URL(path, 'https://library.example'), fetch: api.fetch };
	const data = await load(event as unknown as Event);
	return { data, api };
}

describe('the catalogue page', () => {
	it('shows the catalogue and the filters it offers', async () => {
		const { data, api } = await visit('/');

		expect(data.found).toEqual({ items: [verbs, fractions], total: 2, page: 1, pages: 1 });
		expect(data.filters).toEqual(offered);
		expect(api.asked).toEqual(['/api/catalogue', '/api/catalogue/filters']);
	});

	it('asks the catalogue for the search and filters chosen, nothing else', async () => {
		const { data, api } = await visit(
			'/?q=verbs&language=en-GB&subject=english&age=9&country=GB&year=Year+5&topic=Verb+forms&page=2&utm_source=mail'
		);

		expect(api.asked[0]).toBe(
			'/api/catalogue?q=verbs&language=en-GB&subject=english&age=9&country=GB&year=Year+5&topic=Verb+forms&page=2'
		);
		expect(data.query).toEqual({
			q: 'verbs',
			language: 'en-GB',
			subject: 'english',
			age: '9',
			country: 'GB',
			year: 'Year 5',
			topic: 'Verb forms',
			page: '2'
		});
	});

	it('asks the catalogue for the Selection alone', async () => {
		const { data, api } = await visit('/?selection=true&q=fractions');

		expect(api.asked[0]).toBe('/api/catalogue?q=fractions&selection=true');
		expect(data.query).toEqual({ q: 'fractions', selection: 'true' });
	});

	it('leaves out the filters left empty', async () => {
		const { api } = await visit('/?q=&language=fr&age=');

		expect(api.asked[0]).toBe('/api/catalogue?language=fr');
	});

	it('says a filter is malformed rather than failing', async () => {
		const api = fakeApi();
		api.failure = { status: 400, error: 'bad_filter' };

		const { data } = await visit('/?age=nine', api);

		expect(data.found).toBeNull();
		expect(data.badFilter).toBe(true);
	});

	it('answers 503 when the catalogue is unavailable', async () => {
		const api = fakeApi();
		api.failure = { status: 503, error: 'unavailable' };

		await expect(visit('/', api)).rejects.toMatchObject({ status: 503 });
	});
});
