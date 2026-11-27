import { error, fail } from '@sveltejs/kit';
import { Unavailable } from '#lib/server/catalogue.ts';
import { type MinuteJarAccount, Refused, SESSION_COOKIE, accounts } from '#lib/server/accounts.ts';
import type { Actions, PageServerLoad } from './$types';

const CHOICES: MinuteJarAccount[] = ['same_as_subscription', 'separate'];

/** A text field of the form, empty when missing. */
function text(form: FormData, name: string): string {
	const value = form.get(name);
	return typeof value === 'string' ? value : '';
}

export const load = (async ({ url, fetch, cookies }) => {
	const signInFailed = url.searchParams.get('sign_in') === 'failed';
	const session = cookies.get(SESSION_COOKIE);
	const signedOut = { signedIn: false, signInFailed, account: null, strong: false };
	if (!session) return signedOut;
	try {
		const state = await accounts(fetch).session(session);
		if (!state) {
			cookies.delete(SESSION_COOKIE, { path: '/' });
			return signedOut;
		}
		return {
			signedIn: true,
			signInFailed,
			account: state.account,
			strong: state.strong_sign_in_until !== null
		};
	} catch {
		error(503, 'Accounts are unavailable');
	}
}) satisfies PageServerLoad;

/** The refusal to show, or the error page when the service cannot answer. */
function refusal(failure: unknown, values: Record<string, unknown>) {
	if (failure instanceof Refused) return fail(failure.status, { refused: failure.code, ...values });
	if (failure instanceof Unavailable) error(503, 'Accounts are unavailable');
	throw failure;
}

export const actions = {
	open: async ({ request, fetch, cookies }) => {
		const form = await request.formData();
		const pseudonym = text(form, 'pseudonym');
		const choice = text(form, 'minute_jar_account') as MinuteJarAccount;
		const values = { pseudonym, choice };
		const session = cookies.get(SESSION_COOKIE);
		if (!session) return fail(401, { refused: 'signed_out', ...values });
		if (!CHOICES.includes(choice)) return fail(400, { refused: 'choice_required', ...values });
		try {
			await accounts(fetch).open(session, {
				pseudonym,
				adult_and_parent: form.get('adult_and_parent') === 'on',
				minute_jar_account: choice
			});
			return { opened: true };
		} catch (failure) {
			return refusal(failure, values);
		}
	},

	pseudonym: async ({ request, fetch, cookies }) => {
		const form = await request.formData();
		const pseudonym = text(form, 'pseudonym');
		const session = cookies.get(SESSION_COOKIE);
		if (!session) return fail(401, { refused: 'signed_out', pseudonym });
		try {
			await accounts(fetch).rename(session, pseudonym);
			return { renamed: true };
		} catch (failure) {
			return refusal(failure, { pseudonym });
		}
	}
} satisfies Actions;
