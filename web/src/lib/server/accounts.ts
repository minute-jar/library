// The library service's accounts API (`docs/specs/accounts-api.md`), as the
// website's server reads it. The session lives in an HttpOnly cookie of the
// browser and travels to the API as a bearer token; the browser never calls
// the API itself.

import { fail } from '@sveltejs/kit';
import { LIBRARY_API } from '$app/env/private';
import { Refused, caller } from './api.ts';

/** The cookie holding the session. */
export const SESSION_COOKIE = 'library_session';
/** The cookie holding a started sign-in's state and the visitor's language. */
export const SIGN_IN_COOKIE = 'library_sign_in';

/** An action's refusal without a session, keeping what the form held. */
export function signedOut<Values extends Record<string, unknown> = Record<never, never>>(
	values: Values = {} as Values
) {
	return fail(401, { refused: 'signed_out', ...values });
}

export type MinuteJarAccount = 'same_as_subscription' | 'separate';

/** A library account, as its owner sees it. */
export type Account = {
	pseudonym: string;
	minute_jar_account: MinuteJarAccount;
	opened_at: string;
};

/** What the API knows of a session. */
export type SessionState = {
	/** Until when the sign-in is strong and recent enough to open an account. */
	strong_sign_in_until: string | null;
	account: Account | null;
};

export type Opening = {
	pseudonym: string;
	adult_and_parent: boolean;
	minute_jar_account: MinuteJarAccount;
};

export { Refused };

export function accounts(fetch: typeof globalThis.fetch, root: string = LIBRARY_API) {
	const call = caller(fetch, root, 'accounts');

	return {
		startSignIn(strong: boolean) {
			return call<{ state: string; authorization_url: string }>('POST', '/api/sign-in', {
				body: { strong }
			});
		},
		finishSignIn(state: string, code: string) {
			return call<{ session: string; expires_at: string }>('POST', '/api/sign-in/callback', {
				body: { state, code }
			});
		},
		/** The session's state, or null once it has ended or expired. */
		async session(session: string): Promise<SessionState | null> {
			try {
				return await call<SessionState>('GET', '/api/session', { session });
			} catch (failure) {
				if (failure instanceof Refused && failure.code === 'signed_out') return null;
				throw failure;
			}
		},
		signOut(session: string) {
			return call<void>('DELETE', '/api/session', { session });
		},
		open(session: string, opening: Opening) {
			return call<Account>('POST', '/api/account', { session, body: opening });
		},
		rename(session: string, pseudonym: string) {
			return call<Account>('PUT', '/api/account/pseudonym', { session, body: { pseudonym } });
		},
		/** Closes the library account; every session of its Minute Jar account ends. */
		close(session: string) {
			return call<void>('DELETE', '/api/account', { session });
		}
	};
}
