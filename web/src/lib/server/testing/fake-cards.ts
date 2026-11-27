// A stand-in for the card routes of the accounts API and the payment
// provider, over the fake accounts: a check started is finished by the test
// (or by the fake provider's page in Playwright).

import type { FakeAccounts } from './fake-accounts.ts';
import type { Card } from '../cards.ts';

export type FakeCards = {
	/** The card routes' answers; null for another path. */
	fetch: (input: Request) => Promise<Response> | null;
	/** Minute Jar accounts that pay a subscription with a card. */
	subscribers: Set<string>;
	/** The checks started, by reference, with their outcome. */
	checks: Map<string, 'pending' | 'verified' | 'refused'>;
	/** The verified cards, by Minute Jar account. */
	cards: Map<string, Card>;
	/** Where the fake provider takes the card. */
	provider: string;
};

const json = (body: unknown, status = 200) => Response.json(body, { status });
const refused = (status: number, error: string) => json({ error }, status);

export function fakeCards(accounts: FakeAccounts, provider = 'https://pay.example'): FakeCards {
	const pending = new Map<string, string>();
	let next = 0;

	function answer(request: Request): Response {
		const { pathname } = new URL(request.url);
		const bearer = request.headers.get('authorization')?.replace(/^Bearer /, '') ?? '';
		const subject = accounts.sessions.get(bearer);
		if (!subject) return refused(401, 'signed_out');
		const account = accounts.accounts.get(subject);
		if (!account) return refused(404, 'no_account');
		const card = fake.cards.get(subject);

		if (pathname === '/api/account/card' && request.method === 'POST') {
			if (!accounts.strong) return refused(403, 'strong_sign_in_required');
			if (card) return refused(409, 'card_verified');
			if (account.minute_jar_account === 'same_as_subscription' && fake.subscribers.has(subject)) {
				const verified: Card = { verified_by: 'subscription', verified_at: '2026-11-27T10:00:00Z' };
				fake.cards.set(subject, verified);
				return json({ card: verified });
			}
			const check = `seti_${++next}`;
			fake.checks.set(check, 'pending');
			pending.set(subject, check);
			return json({ check_url: `${fake.provider}/setup/${check}` }, 202);
		}
		if (pathname === '/api/account/card/check' && request.method === 'POST') {
			if (card) return json({ card });
			const check = pending.get(subject);
			if (!check) return refused(404, 'no_card_check');
			const outcome = fake.checks.get(check);
			if (outcome === 'pending') return refused(409, 'card_check_pending');
			pending.delete(subject);
			if (outcome === 'refused') return refused(400, 'card_refused');
			const verified: Card = { verified_by: 'check', verified_at: '2026-11-27T10:02:00Z' };
			fake.cards.set(subject, verified);
			return json({ card: verified });
		}
		if (pathname === '/api/account/card' && request.method === 'GET') {
			return card ? json(card) : refused(404, 'no_card');
		}
		return refused(404, 'not_found');
	}

	const fake: FakeCards = {
		subscribers: new Set(),
		checks: new Map(),
		cards: new Map(),
		provider,
		fetch(request) {
			const { pathname } = new URL(request.url);
			const ours = pathname === '/api/account/card' || pathname.startsWith('/api/account/card/');
			return ours ? Promise.resolve(answer(request)) : null;
		}
	};
	return fake;
}
