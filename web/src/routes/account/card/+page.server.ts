// The card of a library account (stories S7 and S8): the subscription's card
// when the account is on the subscription's Minute Jar account, otherwise a
// zero-amount check at the payment provider, which sends the browser back here.

import { error, fail, redirect } from '@sveltejs/kit';
import { localizeHref } from '#lib/paraglide/runtime.js';
import { Unavailable } from '#lib/server/catalogue.ts';
import { Refused, SESSION_COOKIE, accounts } from '#lib/server/accounts.ts';
import { type Card, type Verification, cards } from '#lib/server/cards.ts';
import type { Actions, PageServerLoad } from './$types';

export const load = (async ({ fetch, cookies }) => {
	const session = cookies.get(SESSION_COOKIE);
	if (!session) redirect(303, localizeHref('/account'));
	let state: Awaited<ReturnType<ReturnType<typeof accounts>['session']>>;
	try {
		state = await accounts(fetch).session(session);
	} catch {
		error(503, 'Accounts are unavailable');
	}
	if (!state?.account) redirect(303, localizeHref('/account'));

	// Reading the check's result also answers a card already verified.
	let card: Card | null = null;
	let outcome: string | null = null;
	try {
		card = await cards(fetch).finish(session);
	} catch (failure) {
		if (failure instanceof Unavailable) error(503, 'Cards are unavailable');
		if (!(failure instanceof Refused)) throw failure;
		outcome = failure.code === 'no_card_check' ? null : failure.code;
	}
	return {
		card,
		subscription: state.account.minute_jar_account === 'same_as_subscription',
		strong: state.strong_sign_in_until !== null,
		outcome
	};
}) satisfies PageServerLoad;

export const actions = {
	verify: async ({ fetch, cookies }) => {
		const session = cookies.get(SESSION_COOKIE);
		if (!session) return fail(401, { refused: 'signed_out' });
		let verification: Verification;
		try {
			verification = await cards(fetch).verify(session);
		} catch (failure) {
			if (failure instanceof Refused) return fail(failure.status, { refused: failure.code });
			if (failure instanceof Unavailable) error(503, 'Cards are unavailable');
			throw failure;
		}
		if ('card' in verification) return { verified: true };
		// The provider's address comes from the service's own configuration.
		redirect(303, verification.check_url, { external: true });
	}
} satisfies Actions;
