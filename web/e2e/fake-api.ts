// The catalogue API's answers for the smoke test, on 127.0.0.1:4174.

import { createServer } from 'node:http';
import { fakeApi } from '../src/lib/server/testing/fake-api.ts';

const api = fakeApi();

createServer((request, response) => {
	void api.fetch(new URL(request.url ?? '/', 'http://127.0.0.1:4174')).then(async (answer) => {
		response.writeHead(answer.status, { 'content-type': 'application/json' });
		response.end(await answer.text());
	});
}).listen(4174, '127.0.0.1');
