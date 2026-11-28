// A stand-in for the library service's catalogue API (`docs/specs/catalogue-api.md`):
// it answers what a test sets and remembers what it was asked.

import type { Entry, Filters, Page } from '../catalogue.ts';
import type { ItemPage, Ratings } from '../items.ts';

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

/** The item page of the irregular verbs, at version 2 of 2. */
export const verbsPage: ItemPage = {
	...verbs,
	signals: { families: 3, ratings: [0, 0, 1, 0, 2] },
	content: {
		questions: [
			{ type: 'typed', prompt: "Past of 'go'", answers: ['went'] },
			{ type: 'choice', prompt: "Past of 'see'", choices: ['saw', 'seed'], answers: ['saw'] }
		],
		lessons: [{ title: 'Strong verbs', text: 'Their vowel changes in the past.' }]
	},
	media: [],
	editorial: null,
	versions: [
		{ version: 2, published_at: '2026-11-20T18:00:00Z' },
		{ version: 1, published_at: '2026-11-18T09:00:00Z' }
	]
};

/** The item page of a derived work of version 1 of the irregular verbs, by another author. */
export const moreVerbs: ItemPage = {
	...verbs,
	item: '00112233445566778899aabbccddeeff',
	version: 1,
	title: 'More irregular verbs',
	author: { pseudonym: 'Lark', editorial: false },
	derived_from: { item: verbs.item, version: 1, pseudonym: 'Owl' },
	published_at: '2026-11-24T08:00:00Z',
	content: { questions: [{ type: 'typed', prompt: "Past of 'run'", answers: ['ran'] }] },
	media: [],
	editorial: null,
	versions: [{ version: 1, published_at: '2026-11-24T08:00:00Z' }]
};

/** The item page of the fractions, shared by the editorial account. */
export const fractionsPage: ItemPage = {
	...fractions,
	content: { questions: [{ type: 'typed', prompt: 'Simplifier 2/4', answers: ['1/2'] }] },
	media: [],
	editorial: {
		ai_generated: true,
		read: { by: 'moderator', handle: 'Heron', at: '2026-11-20T09:00:00Z' }
	},
	versions: [{ version: 1, published_at: '2026-11-21T08:00:00Z' }]
};

export const verbsRatings: Ratings = {
	item: verbs.item,
	ratings: [0, 0, 1, 0, 2],
	reviews: [{ stars: 5, text: 'Learnt in a week.' }]
};

/** The item file of version 2 of the irregular verbs, as the API answers it. */
export const verbsFile =
	'{"contract":1,"kind":"item_file","keys":{"payload":"{}","signature":"00","key":"00"},"item":{"payload":"{}","signature":"00","key":"00"},"media":[]}';

export type FakeApi = {
	/** The catalogue API's answers, as `fetch` gives them. */
	fetch: typeof fetch;
	/** The paths and query strings asked for, in order. */
	asked: string[];
	/** What `/api/catalogue` answers next. */
	page: Page;
	/** What `/api/catalogue/filters` answers next. */
	filters: Filters;
	/** The item pages, by item. */
	items: Record<string, ItemPage>;
	/** The ratings, by item. */
	ratings: Record<string, Ratings>;
	/** The item files, by item and version (`<item>/<version>`). */
	files: Record<string, string>;
	/** A status `/api/catalogue` and `/api/items/...` answer instead, with its error code. */
	failure: { status: number; error: string } | null;
};

export function fakeApi(): FakeApi {
	const api: FakeApi = {
		asked: [],
		page: { items: [verbs, fractions], total: 2, page: 1, pages: 1 },
		filters: offered,
		items: {
			[verbs.item]: verbsPage,
			[moreVerbs.item]: moreVerbs,
			[fractions.item]: fractionsPage
		},
		ratings: {
			[fractions.item]: { item: fractions.item, ratings: [0, 0, 0, 0, 0], reviews: [] },
			[verbs.item]: verbsRatings,
			[moreVerbs.item]: { item: moreVerbs.item, ratings: [0, 0, 0, 0, 0], reviews: [] }
		},
		files: { [`${verbs.item}/2`]: verbsFile, [`${verbs.item}/1`]: verbsFile },
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
			const asked = url.pathname.match(
				/^\/api\/items\/([^/]+)(?:\/(ratings)|\/versions\/([^/]+)\/file)?$/
			);
			if (asked) return Promise.resolve(itemAnswer(api, asked[1], asked[2], asked[3]));
			return Promise.resolve(Response.json({ error: 'not_found' }, { status: 404 }));
		}
	};
	return api;
}

function itemAnswer(api: FakeApi, item: string, ratings?: string, version?: string): Response {
	const { failure } = api;
	if (failure) return Response.json({ error: failure.error }, { status: failure.status });
	if (!api.items[item]) return Response.json({ error: 'unknown_item' }, { status: 404 });
	if (ratings) return Response.json(api.ratings[item]);
	if (version === undefined) return Response.json(api.items[item]);
	const file = api.files[`${item}/${version}`];
	if (file === undefined) return Response.json({ error: 'unknown_version' }, { status: 404 });
	return new Response(file, {
		headers: {
			'content-type': 'application/json',
			'content-disposition': `attachment; filename="minute-jar-${item}-${version}.json"`
		}
	});
}
