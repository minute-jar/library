// A stand-in for the suggestions routes of the library service, over the
// fake accounts, and for the share route as it takes a new version from
// accepted suggestions: a test sets the suggestions each account received.

import type { FakeAccounts } from './fake-accounts.ts';
import type { Share } from '../shares.ts';
import type { ItemSuggestions, Prepared } from '../suggestions.ts';

export type FakeSuggestions = {
	/** The suggestions routes' answers; null for another path. */
	fetch: (input: Request) => Promise<Response> | null;
	/** The suggestions received, by Minute Jar account. */
	received: Map<string, ItemSuggestions[]>;
	/** What any account without suggestions of its own received, for the smoke tests. */
	anyone: ItemSuggestions[];
	/** The new versions shared, in order, by Minute Jar account. */
	shared: Map<string, Prepared[]>;
};

const json = (body: unknown, status = 200) => Response.json(body, { status });
const refused = (status: number, error: string) => json({ error }, status);

export function fakeSuggestions(accounts: FakeAccounts): FakeSuggestions {
	async function answer(request: Request): Promise<Response> {
		const { pathname } = new URL(request.url);
		const bearer = request.headers.get('authorization')?.replace(/^Bearer /, '') ?? '';
		const subject = accounts.sessions.get(bearer);
		if (!subject) return refused(401, 'signed_out');
		const received = fake.received.get(subject) ?? fake.anyone;

		if (pathname === '/api/account/suggestions' && request.method === 'GET') {
			return json({ items: received });
		}
		const accepting = pathname.match(/^\/api\/account\/items\/([^/]+)\/suggestions\/accept$/);
		if (accepting && request.method === 'POST') {
			const suggestions = received.find((item) => item.item === accepting[1]);
			if (!suggestions) return refused(404, 'unknown_item');
			const { accepted } = (await request.json()) as {
				accepted: { question: number; answers: string[] }[];
			};
			if (accepted.length === 0) return refused(400, 'bad_suggestion');
			for (const { question, answers } of accepted) {
				const offered = suggestions.questions.find((asked) => asked.question === question);
				const known = (answer: string) => offered?.answers.some((one) => one.answer === answer);
				if (answers.length === 0 || !answers.every(known)) return refused(409, 'not_suggested');
			}
			return json({
				prepared: {
					item: suggestions.item,
					type: 'bank',
					title: suggestions.title,
					content: { accepted },
					media: []
				}
			});
		}
		if (pathname === '/api/shares' && request.method === 'POST') {
			const prepared = (await request.json()) as Prepared;
			const shared = [...(fake.shared.get(subject) ?? []), prepared];
			fake.shared.set(subject, shared);
			const version = (received.find((item) => item.item === prepared.item)?.version ?? 0) + 1;
			const share: Share = {
				share: `share-${shared.length}`,
				item: prepared.item,
				title: prepared.title,
				outcome: 'published',
				version,
				reasons: [],
				shared_at: '2026-11-27T10:00:00Z'
			};
			return json(share, 201);
		}
		return refused(404, 'not_found');
	}

	const fake: FakeSuggestions = {
		received: new Map(),
		anyone: [],
		shared: new Map(),
		fetch(request) {
			const { pathname } = new URL(request.url);
			const ours =
				pathname === '/api/account/suggestions' ||
				pathname === '/api/shares' ||
				/^\/api\/account\/items\/[^/]+\/suggestions\/accept$/.test(pathname);
			return ours ? answer(request) : null;
		}
	};
	return fake;
}
