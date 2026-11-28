// Author profiles (story S19, `docs/specs/catalogue-api.md`, Author
// profiles), as the website's server reads them: an author's items and how
// many families use them, the same for everyone, and the author's own view,
// which adds the suggestions received.

import { LIBRARY_API } from '$app/env/private';
import { Refused } from './accounts.ts';
import { type Entry, Unavailable } from './catalogue.ts';
import { NotFound } from './items.ts';

/** An author's profile, as anyone sees it. */
export type Profile = {
	pseudonym: string;
	/** The operator's editorial account (ADR-0002). */
	editorial: boolean;
	/** The distinct families that imported at least one of the items. */
	families: number;
	/** The items the catalogue shows, newest first. */
	items: Entry[];
};

/** The suggestions an item received, counted, for its author's eyes only. */
export type Received = {
	/** The distinct families that ever suggested answers for it. */
	families: number;
	/** The suggested answers still waiting against its latest version. */
	answers: number;
};

/** The author's own view of their profile. */
export type OwnProfile = Omit<Profile, 'items'> & {
	items: (Entry & { suggestions: Received })[];
};

export function profiles(fetch: typeof globalThis.fetch, root: string = LIBRARY_API) {
	async function call(path: string, session?: string): Promise<Response> {
		let response: Response;
		try {
			response = await fetch(new URL(path, root), {
				headers: session ? { authorization: `Bearer ${session}` } : {}
			});
		} catch (cause) {
			throw new Unavailable('the profiles API is unreachable', { cause });
		}
		if (response.status >= 500) {
			throw new Unavailable(`the profiles API answered ${response.status}`);
		}
		return response;
	}

	return {
		/** The profile of the author holding `pseudonym`; `NotFound` when there is none. */
		async public(pseudonym: string): Promise<Profile> {
			const response = await call(`/api/authors/${encodeURIComponent(pseudonym)}`);
			if (response.status === 404) throw new NotFound(pseudonym);
			if (!response.ok) throw new Unavailable(`the profiles API answered ${response.status}`);
			return (await response.json()) as Profile;
		},
		/** The session's own profile, with the suggestions received. */
		async own(session: string): Promise<OwnProfile> {
			const response = await call('/api/account/profile', session);
			const answer = (await response.json()) as OwnProfile & { error?: string };
			if (!response.ok) throw new Refused(response.status, answer.error ?? 'bad_request');
			return answer;
		}
	};
}
