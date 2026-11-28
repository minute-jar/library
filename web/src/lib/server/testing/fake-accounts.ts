// A stand-in for the library service's accounts API (`docs/specs/accounts-api.md`)
// and its identity provider: sign-ins come back with the code `code-<state>`.

import type { Account, SessionState } from '../accounts.ts';

export type FakeAccounts = {
	/** The accounts API's answers, as `fetch` gives them; null for another path. */
	fetch: (input: Request) => Promise<Response> | null;
	/** The method, path and session of each call, in order. */
	asked: { method: string; path: string; session: string | null; body: unknown }[];
	/** Where the fake provider signs people in. */
	provider: string;
	/** Whether the next sign-ins are strong and recent. */
	strong: boolean;
	/** The sessions handed out, with their Minute Jar account. */
	sessions: Map<string, string>;
	/** The library accounts, by Minute Jar account. */
	accounts: Map<string, Account>;
	/** Opens a session at once on the Minute Jar account `subject`. */
	signedIn(subject: string, account?: Account): string;
};

const json = (body: unknown, status = 200) => Response.json(body, { status });
const refused = (status: number, error: string) => json({ error }, status);

export function fakeAccounts(provider = 'https://id.example'): FakeAccounts {
	const started = new Set<string>();
	const strongSessions = new Set<string>();
	/** The pseudonyms of closed accounts, which stay reserved. */
	const closed = new Set<string>();
	let next = 0;

	function open(subject: string, strong: boolean) {
		const session = `session-${++next}`;
		fake.sessions.set(session, subject);
		if (strong) strongSessions.add(session);
		return session;
	}

	async function answer(request: Request): Promise<Response> {
		const url = new URL(request.url);
		const bearer = request.headers.get('authorization')?.replace(/^Bearer /, '') ?? null;
		const body: unknown =
			request.method === 'GET' ? undefined : await request.json().catch(() => undefined);
		fake.asked.push({ method: request.method, path: url.pathname, session: bearer, body });
		const fields = (body ?? {}) as Record<string, unknown>;

		if (url.pathname === '/api/sign-in') {
			const state = `state-${++next}`;
			started.add(state);
			return json({
				state,
				authorization_url: `${fake.provider}/authorize?state=${state}&strong=${fields.strong === true}`
			});
		}
		if (url.pathname === '/api/sign-in/callback') {
			const state = String(fields.state);
			if (!started.delete(state) || fields.code !== `code-${state}`) {
				return refused(400, 'bad_sign_in');
			}
			return json({
				session: open(`subject-${state}`, fake.strong),
				expires_at: '2026-12-27T10:00:00Z'
			});
		}

		const subject = bearer ? fake.sessions.get(bearer) : undefined;
		if (!bearer || !subject) return refused(401, 'signed_out');
		const account = fake.accounts.get(subject) ?? null;

		if (url.pathname === '/api/session' && request.method === 'GET') {
			const state: SessionState = {
				strong_sign_in_until: strongSessions.has(bearer) ? '2026-11-27T10:15:00Z' : null,
				account
			};
			return json(state);
		}
		if (url.pathname === '/api/session' && request.method === 'DELETE') {
			fake.sessions.delete(bearer);
			return new Response(null, { status: 204 });
		}
		if (url.pathname === '/api/account' && request.method === 'POST') {
			if (!strongSessions.has(bearer)) return refused(403, 'strong_sign_in_required');
			if (account) return refused(409, 'account_exists');
			if (fields.adult_and_parent !== true) return refused(400, 'statement_required');
			const pseudonym = String(fields.pseudonym).trim();
			const taken =
				closed.has(pseudonym.toLowerCase()) ||
				[...fake.accounts.values()].some(
					(other) => other.pseudonym.toLowerCase() === pseudonym.toLowerCase()
				);
			if (pseudonym.length < 3 || pseudonym.includes('@')) return refused(400, 'bad_pseudonym');
			if (taken) return refused(409, 'pseudonym_taken');
			const opened: Account = {
				pseudonym,
				minute_jar_account: fields.minute_jar_account as Account['minute_jar_account'],
				opened_at: '2026-11-27T10:00:00Z',
				show_earlier_pseudonyms: false
			};
			fake.accounts.set(subject, opened);
			return json(opened, 201);
		}
		if (url.pathname === '/api/account' && request.method === 'DELETE') {
			if (!strongSessions.has(bearer)) return refused(403, 'strong_sign_in_required');
			if (!account) return refused(404, 'no_account');
			closed.add(account.pseudonym.toLowerCase());
			fake.accounts.delete(subject);
			for (const [other, owner] of fake.sessions) {
				if (owner === subject) fake.sessions.delete(other);
			}
			return new Response(null, { status: 204 });
		}
		if (url.pathname === '/api/account/earlier-pseudonyms' && request.method === 'PUT') {
			if (!account) return refused(404, 'no_account');
			if (typeof fields.shown !== 'boolean') return refused(400, 'bad_request');
			const chosen = { ...account, show_earlier_pseudonyms: fields.shown };
			fake.accounts.set(subject, chosen);
			return json(chosen);
		}
		if (url.pathname === '/api/account/pseudonym' && request.method === 'PUT') {
			if (!account) return refused(404, 'no_account');
			const pseudonym = String(fields.pseudonym).trim();
			if (pseudonym.length < 3 || pseudonym.includes('@')) return refused(400, 'bad_pseudonym');
			const renamed = { ...account, pseudonym };
			fake.accounts.set(subject, renamed);
			return json(renamed);
		}
		return refused(404, 'not_found');
	}

	const fake: FakeAccounts = {
		asked: [],
		provider,
		strong: true,
		sessions: new Map(),
		accounts: new Map(),
		signedIn(subject, account) {
			if (account) fake.accounts.set(subject, account);
			return open(subject, fake.strong);
		},
		fetch(request) {
			const { pathname } = new URL(request.url);
			const ours = ['/api/sign-in', '/api/session', '/api/account'].some(
				(prefix) => pathname === prefix || pathname.startsWith(`${prefix}/`)
			);
			return ours ? answer(request) : null;
		}
	};
	return fake;
}

/** The cookies of one request, as SvelteKit's `cookies` handles them, and what the answer sets. */
export function fakeCookies(initial: Record<string, string> = {}) {
	const jar = new Map(Object.entries(initial));
	const written: Record<string, { value: string; options: Record<string, unknown> }> = {};
	const cookies = {
		get: (name: string) => jar.get(name),
		getAll: () => [...jar].map(([name, value]) => ({ name, value })),
		set(name: string, value: string, options: Record<string, unknown>) {
			jar.set(name, value);
			written[name] = { value, options };
		},
		delete(name: string, options: Record<string, unknown>) {
			jar.delete(name);
			written[name] = { value: '', options: { ...options, deleted: true } };
		},
		serialize: () => ''
	};
	return { cookies, written };
}
