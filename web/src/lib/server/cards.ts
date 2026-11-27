// The card part of the library service's accounts API
// (`docs/specs/accounts-api.md`, Card), as the website's server reads it.
// The website never sees a card number, a reference or a fingerprint: the
// browser enters the card at the payment provider.

import { LIBRARY_API } from '$app/env/private';
import { Refused } from './accounts.ts';
import { Unavailable } from './catalogue.ts';

/** A library account's verified card, as its owner sees it. */
export type Card = {
	verified_by: 'subscription' | 'check';
	verified_at: string;
};

/** Verifying gives the card at once (the subscription's), or where to check one. */
export type Verification = { card: Card } | { check_url: string };

export function cards(fetch: typeof globalThis.fetch, root: string = LIBRARY_API) {
	async function call<T>(method: string, path: string, session: string): Promise<T> {
		let response: Response;
		try {
			response = await fetch(new URL(path, root), {
				method,
				headers: { authorization: `Bearer ${session}` }
			});
		} catch (cause) {
			throw new Unavailable('the accounts API is unreachable', { cause });
		}
		if (response.status >= 500)
			throw new Unavailable(`the accounts API answered ${response.status}`);
		const answer = (await response.json()) as T & { error?: string };
		if (!response.ok) throw new Refused(response.status, answer.error ?? 'bad_request');
		return answer;
	}

	return {
		/** Uses the subscription's card, or starts a zero-amount check. */
		verify(session: string) {
			return call<Verification>('POST', '/api/account/card', session);
		},
		/** The card once the browser is back from the provider; refused while it is not verified. */
		async finish(session: string): Promise<Card> {
			return (await call<{ card: Card }>('POST', '/api/account/card/check', session)).card;
		}
	};
}
