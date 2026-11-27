// The library service's catalogue API (`docs/specs/catalogue-api.md`), as the
// website's server reads it. The browser never calls the API itself.

import { LIBRARY_API } from '$app/env/private';

export type Curriculum = { country: string; year: string; topic: string };

/** A shared item at its latest version. */
export type Entry = {
	item: string;
	version: number;
	type: 'bank' | 'path';
	title: string;
	licence: string;
	author: { pseudonym: string; editorial: boolean };
	derived_from: { item: string; version: number; pseudonym: string } | null;
	metadata: {
		language: string;
		subject: string;
		ages: { min: number; max: number };
		description: string;
		curriculum: Curriculum | null;
	};
	selection: boolean;
	signals: { families: number; /** How many ratings of 1 to 5 stars. */ ratings: number[] };
	published_at: string;
};

/** One page of the catalogue, as browsed or searched. */
export type Page = { items: Entry[]; total: number; page: number; pages: number };

/** The values the catalogue's items take. */
export type Filters = { languages: string[]; subjects: string[]; curricula: Curriculum[] };

/** The search and filters the API understands, in the order they are asked. */
export const SEARCH = [
	'q',
	'language',
	'subject',
	'age',
	'country',
	'year',
	'topic',
	'page'
] as const;

export type Search = Partial<Record<(typeof SEARCH)[number], string>>;

/** The search and filters a visitor chose, without empty or unknown parameters. */
export function chosen(params: URLSearchParams): Search {
	const search: Search = {};
	for (const name of SEARCH) {
		const value = params.get(name)?.trim();
		if (value) search[name] = value;
	}
	return search;
}

/** The API refused the filters as malformed. */
export class BadFilter extends Error {}

/** The API could not answer. */
export class Unavailable extends Error {}

export function catalogue(fetch: typeof globalThis.fetch, root: string = LIBRARY_API) {
	async function get<T>(path: string): Promise<T> {
		let response: Response;
		try {
			response = await fetch(new URL(path, root));
		} catch (cause) {
			throw new Unavailable('the catalogue API is unreachable', { cause });
		}
		if (response.status === 400) throw new BadFilter('malformed filters');
		if (!response.ok) throw new Unavailable(`the catalogue API answered ${response.status}`);
		return (await response.json()) as T;
	}

	return {
		browse(search: Search): Promise<Page> {
			const query = new URLSearchParams(search).toString();
			return get<Page>(query ? `/api/catalogue?${query}` : '/api/catalogue');
		},
		filters(): Promise<Filters> {
			return get<Filters>('/api/catalogue/filters');
		}
	};
}
