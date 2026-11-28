// The library service's item API (`docs/specs/catalogue-api.md`, Item page,
// and `docs/specs/interface-v1.md`, Item files), as the website's server
// reads it. The browser never calls the API itself.

import { LIBRARY_API } from '$app/env/private';
import type { Entry } from './catalogue.ts';

/** A question, in the bank format; only what the page shows is named. */
export type Question = {
	type: string;
	prompt: string;
	answers?: string[];
	choices?: string[];
	explanation?: string;
	/** The digest of a picture listed in `media`. */
	picture?: string;
	[field: string]: unknown;
};

export type Lesson = { title?: string; text: string; [field: string]: unknown };

/** Where the content comes from: a curriculum, a work, a page. */
export type Source = string | { title: string; url?: string };

export type Content = {
	questions?: Question[];
	lessons?: Lesson[];
	sources?: Source[];
	[field: string]: unknown;
};

export type Medium = { digest: string; type: string; size: number };

export type Version = { version: number; published_at: string };

/** A published comment from a library account, under its pseudonym. */
export type Comment = { comment: string; pseudonym: string; text: string; published_at: string };

/** Who read an editorial item before it was shared, and when. */
export type Read =
	| { by: 'moderator'; /** The moderator's handle. */ handle: string; at: string }
	| { by: 'operator'; at: string };

/** How the editorial account made a version, and who read it (ADR-0002). */
export type Editorial = { ai_generated: boolean; read: Read };

/** A shared item at its latest version, as its page shows it. */
export type ItemPage = Entry & {
	content: Content;
	media: Medium[];
	/** Every version the catalogue shows, newest first. */
	versions: Version[];
	/** Null unless the editorial account shared this version. */
	editorial: Editorial | null;
	/** The published comments, oldest first. */
	comments?: Comment[];
	/**
	 * The pseudonym under which the author's profile is found now, which may
	 * differ from the one this version was published under; null for an
	 * item no library account shared.
	 */
	profile?: string | null;
};

/** How an item is rated now, with its published short reviews. */
export type Ratings = {
	item: string;
	ratings: number[];
	reviews: { stars: number; text: string }[];
};

/** The catalogue does not show this item or version. */
export class NotFound extends Error {}

/** The API could not answer. */
export class Unavailable extends Error {}

export function items(fetch: typeof globalThis.fetch, root: string = LIBRARY_API) {
	async function get(path: string): Promise<Response> {
		let response: Response;
		try {
			response = await fetch(new URL(path, root));
		} catch (cause) {
			throw new Unavailable('the item API is unreachable', { cause });
		}
		if (response.status === 404) throw new NotFound(path);
		if (!response.ok) throw new Unavailable(`the item API answered ${response.status}`);
		return response;
	}
	const item = (id: string) => `/api/items/${encodeURIComponent(id)}`;

	return {
		async page(id: string): Promise<ItemPage> {
			return (await (await get(item(id))).json()) as ItemPage;
		},
		async ratings(id: string): Promise<Ratings> {
			return (await (await get(`${item(id)}/ratings`)).json()) as Ratings;
		},
		/** The item file of one version, as the API answers it. */
		file(id: string, version: string): Promise<Response> {
			return get(`${item(id)}/versions/${encodeURIComponent(version)}/file`);
		}
	};
}
