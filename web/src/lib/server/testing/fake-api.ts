// A stand-in for the library service's catalogue API (`docs/specs/catalogue-api.md`):
// it answers what a test sets and remembers what it was asked.

import type { Entry, Filters, Page } from '../catalogue.ts';

export const verbs: Entry = {
	item: '0123456789abcdef0123456789abcdef',
	version: 2,
	type: 'bank',
	title: 'Irregular verbs, part 1',
	licence: 'CC-BY-SA-4.0',
	author: { pseudonym: 'Owl', editorial: false },
	derived_from: null,
	metadata: {
		language: 'en-GB',
		subject: 'english',
		ages: { min: 9, max: 11 },
		description: 'The fifty most common irregular verbs.',
		curriculum: { country: 'GB', year: 'Year 5', topic: 'Verb forms' }
	},
	selection: false,
	signals: { families: 0, ratings: [0, 0, 0, 0, 0] },
	published_at: '2026-11-20T18:00:00Z'
};

export const fractions: Entry = {
	item: 'fedcba9876543210fedcba9876543210',
	version: 1,
	type: 'path',
	title: 'Les fractions, CM1',
	licence: 'CC-BY-SA-4.0',
	author: { pseudonym: 'Minute Jar', editorial: true },
	derived_from: null,
	metadata: {
		language: 'fr-FR',
		subject: 'mathematics',
		ages: { min: 9, max: 10 },
		description: 'Comparer et ranger des fractions simples.',
		curriculum: null
	},
	selection: true,
	signals: { families: 0, ratings: [0, 0, 0, 0, 0] },
	published_at: '2026-11-21T08:00:00Z'
};

export const offered: Filters = {
	languages: ['en-GB', 'fr-FR'],
	subjects: ['english', 'mathematics'],
	curricula: [{ country: 'GB', year: 'Year 5', topic: 'Verb forms' }]
};

export type FakeApi = {
	/** The catalogue API's answers, as `fetch` gives them. */
	fetch: typeof fetch;
	/** The paths and query strings asked for, in order. */
	asked: string[];
	/** What `/api/catalogue` answers next. */
	page: Page;
	/** What `/api/catalogue/filters` answers next. */
	filters: Filters;
	/** A status `/api/catalogue` answers instead, with its error code. */
	failure: { status: number; error: string } | null;
};

export function fakeApi(): FakeApi {
	const api: FakeApi = {
		asked: [],
		page: { items: [verbs, fractions], total: 2, page: 1, pages: 1 },
		filters: offered,
		failure: null,
		fetch: (input) => {
			const url = new URL(input instanceof Request ? input.url : input.toString());
			api.asked.push(url.pathname + url.search);
			if (url.pathname === '/api/catalogue') {
				const { failure } = api;
				return Promise.resolve(
					failure
						? Response.json({ error: failure.error }, { status: failure.status })
						: Response.json(api.page)
				);
			}
			if (url.pathname === '/api/catalogue/filters') {
				return Promise.resolve(Response.json(api.filters));
			}
			return Promise.resolve(Response.json({ error: 'not_found' }, { status: 404 }));
		}
	};
	return api;
}
