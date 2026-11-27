// The catalogue and accounts APIs' answers for the smoke tests, on
// 127.0.0.1:4174, and a fake identity provider at `/authorize` that signs in
// at once and sends the browser back to the website's callback page.

import { createServer } from 'node:http';
import { fakeApi } from '../src/lib/server/testing/fake-api.ts';
import { fakeAccounts } from '../src/lib/server/testing/fake-accounts.ts';

const root = 'http://127.0.0.1:4174';
const api = fakeApi();
const accounts = fakeAccounts(root);

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
	const chunks: Buffer[] = [];
	request.on('data', (chunk: Buffer) => chunks.push(chunk));
	request.on('end', () => {
		const body = chunks.length > 0 ? Buffer.concat(chunks) : undefined;
		const asked = new Request(url, {
			method: request.method,
			headers: request.headers as Record<string, string>,
			body
		});
		void (accounts.fetch(asked) ?? api.fetch(asked)).then(async (answer) => {
			response.writeHead(answer.status, Object.fromEntries(answer.headers));
			response.end(await answer.text());
		});
	});
}).listen(4174, '127.0.0.1');
