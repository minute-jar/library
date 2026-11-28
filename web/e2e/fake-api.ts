// The catalogue, accounts, sharing, author's items, profiles, suggestions, comments and moderation APIs' answers for the smoke tests, on
// 127.0.0.1:4174, a fake identity provider at `/authorize` that signs in
// at once and sends the browser back to the website's callback page, and a
// fake payment provider at `/setup/<check>` that verifies the card at once
// and sends the browser back to the website's card page.

import { createServer } from 'node:http';
import { fakeApi, verbs } from '../src/lib/server/testing/fake-api.ts';
import { fakeAccounts } from '../src/lib/server/testing/fake-accounts.ts';
import { fakeCards } from '../src/lib/server/testing/fake-cards.ts';
import { fakeShares } from '../src/lib/server/testing/fake-shares.ts';
import { fakeComments } from '../src/lib/server/testing/fake-comments.ts';
import { fakeModeration } from '../src/lib/server/testing/fake-moderation.ts';
import { fakeSuggestions } from '../src/lib/server/testing/fake-suggestions.ts';
import { fakeAuthored } from '../src/lib/server/testing/fake-authored.ts';
import { fakeProfiles } from '../src/lib/server/testing/fake-profiles.ts';
import type { AuthoredItem } from '../src/lib/server/authored.ts';

const root = 'http://127.0.0.1:4174';
const verbsOfTheAuthor: AuthoredItem = {
	item: '0123456789abcdef0123456789abcdef',
	title: 'Irregular verbs, part 2',
	withdrawal: null,
	versions: [
		{ version: 2, published_at: '2026-11-27T10:05:00Z', withdrawal: null },
		{ version: 1, published_at: '2026-11-27T10:00:00Z', withdrawal: null }
	]
};
const api = fakeApi();
const accounts = fakeAccounts(root);
const cards = fakeCards(accounts, root);
const shares = fakeShares(accounts);
const comments = fakeComments(accounts, api);
const moderation = fakeModeration(accounts);
const suggestions = fakeSuggestions(accounts);
// Whoever signs in received suggestions for one item of theirs.
suggestions.anyone = [
	{
		item: '0123456789abcdef0123456789abcdef',
		title: 'Irregular verbs',
		version: 1,
		questions: [
			{
				question: 1,
				prompt: "Past of 'go'",
				answers: [
					{ answer: 'goed', families: 2 },
					{ answer: 'has gone', families: 1 }
				]
			}
		]
	}
];
const authored = fakeAuthored(accounts);
// Whoever opens an account has published one item.
authored.everyone = [verbsOfTheAuthor];
const profiles = fakeProfiles(accounts);
// Whoever opens an account has one item on their profile, with suggestions.
profiles.everyone = [
	{
		...verbs,
		signals: { families: 3, ratings: [0, 0, 0, 0, 0] },
		suggestions: { families: 2, answers: 3 }
	}
];
// Whoever signs in moderates: the operator's naming is the service's concern.
moderation.anyoneModerates = true;

createServer((request, response) => {
	const url = new URL(request.url ?? '/', root);
	if (url.pathname === '/authorize') {
		const state = url.searchParams.get('state') ?? '';
		const back = new URL('http://127.0.0.1:4173/sign-in/callback');
		back.search = new URLSearchParams({ code: `code-${state}`, state }).toString();
		response.writeHead(303, { location: back.toString() });
		response.end();
		return;
	}
	if (url.pathname.startsWith('/setup/')) {
		cards.checks.set(url.pathname.slice('/setup/'.length), 'verified');
		response.writeHead(303, { location: 'http://127.0.0.1:4173/account/card' });
		response.end();
		return;
	}
	const chunks: Buffer[] = [];
	request.on('data', (chunk: Buffer) => chunks.push(chunk));
	request.on('end', () => {
		const body = chunks.length > 0 ? Buffer.concat(chunks) : undefined;
		const asked = new Request(url, {
			method: request.method,
			headers: request.headers as Record<string, string>,
			body
		});
		void (
			suggestions.fetch(asked) ??
			comments.fetch(asked) ??
			shares.fetch(asked) ??
			authored.fetch(asked) ??
			profiles.fetch(asked) ??
			cards.fetch(asked) ??
			moderation.fetch(asked) ??
			accounts.fetch(asked) ??
			api.fetch(asked)
		).then(async (answer) => {
			response.writeHead(answer.status, Object.fromEntries(answer.headers));
			response.end(await answer.text());
		});
	});
}).listen(4174, '127.0.0.1');
