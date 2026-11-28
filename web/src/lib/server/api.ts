// One call to the library service's own JSON API (accounts, sharing,
// comments, moderation...), as the website's server makes it: the session
// travels as a bearer token, a refusal's fixed code becomes `Refused`, and an
// unreachable or failing API becomes `Unavailable`.

import { Unavailable } from './catalogue.ts';

/** The API refused the request with a fixed code (`bad_pseudonym`, `signed_out`...). */
export class Refused extends Error {
	constructor(
		readonly status: number,
		readonly code: string
	) {
		super(`the library API refused: ${code}`);
	}
}

export type Call = <T>(
	method: string,
	path: string,
	options?: { session?: string; body?: unknown }
) => Promise<T>;

/** Calls the API at `root` through `fetch`; `name` says which API failed. */
export function caller(fetch: typeof globalThis.fetch, root: string, name: string): Call {
	return async function call<T>(
		method: string,
		path: string,
		{ session, body }: { session?: string; body?: unknown } = {}
	): Promise<T> {
		const headers: Record<string, string> = {};
		if (session) headers.authorization = `Bearer ${session}`;
		if (body !== undefined) headers['content-type'] = 'application/json';
		let response: Response;
		try {
			response = await fetch(new URL(path, root), {
				method,
				headers,
				body: body === undefined ? undefined : JSON.stringify(body)
			});
		} catch (cause) {
			throw new Unavailable(`the ${name} API is unreachable`, { cause });
		}
		if (response.status === 204) return undefined as T;
		if (response.status >= 500)
			throw new Unavailable(`the ${name} API answered ${response.status}`);
		const answer = (await response.json()) as T & { error?: string };
		if (!response.ok) throw new Refused(response.status, answer.error ?? 'bad_request');
		return answer;
	};
}
