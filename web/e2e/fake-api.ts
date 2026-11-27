// The catalogue, accounts, sharing and moderation APIs' answers for the smoke tests, on
// 127.0.0.1:4174, a fake identity provider at `/authorize` that signs in
// at once and sends the browser back to the website's callback page, and a
// fake payment provider at `/setup/<check>` that verifies the card at once
// and sends the browser back to the website's card page.

import { createServer } from 'node:http';
import { fakeApi } from '../src/lib/server/testing/fake-api.ts';
import { fakeAccounts } from '../src/lib/server/testing/fake-accounts.ts';
import { fakeCards } from '../src/lib/server/testing/fake-cards.ts';
import { fakeShares } from '../src/lib/server/testing/fake-shares.ts';
import { fakeModeration } from '../src/lib/server/testing/fake-moderation.ts';

const root = 'http://127.0.0.1:4174';
const api = fakeApi();
const accounts = fakeAccounts(root);
const cards = fakeCards(accounts, root);
const shares = fakeShares(accounts);
const moderation = fakeModeration(accounts);
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
			shares.fetch(asked) ??
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
