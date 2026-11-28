// Author profiles (story S19, `docs/specs/catalogue-api.md`, Author
// profiles), as the website's server reads them: an author's items and how
// many families use them, the same for everyone, and the author's own view,
// which adds the suggestions received.

import { LIBRARY_API } from '$app/env/private';
import { Refused, caller } from './api.ts';
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
	const call = caller(fetch, root, 'profiles');

	return {
		/** The profile of the author holding `pseudonym`; `NotFound` when there is none. */
		async public(pseudonym: string): Promise<Profile> {
			try {
				return await call<Profile>('GET', `/api/authors/${encodeURIComponent(pseudonym)}`);
			} catch (failure) {
				if (failure instanceof Refused && failure.status === 404) throw new NotFound(pseudonym);
				if (failure instanceof Refused)
					throw new Unavailable(`the profiles API answered ${failure.status}`);
				throw failure;
			}
		},
		/** The session's own profile, with the suggestions received. */
		own(session: string): Promise<OwnProfile> {
			return call<OwnProfile>('GET', '/api/account/profile', { session });
		}
	};
}
