// Suggestions to authors (story S18, `docs/specs/interface-v1.md`,
// Suggestions), as the website's server reads them: the answers families
// suggested for the author's items, counted by family and never telling
// who, and their acceptance into a new version, which the share route
// reviews like any share.

import { LIBRARY_API } from '$app/env/private';
import { caller } from './api.ts';
import type { Share } from './shares.ts';

/** An answer and how many families suggested it. */
export type SuggestedAnswer = { answer: string; families: number };

/** The answers suggested for one question of the latest version. */
export type QuestionSuggestions = {
	/** The question's place in the latest version, from 1. */
	question: number;
	prompt: string | null;
	answers: SuggestedAnswer[];
};

/** The suggestions of one of the author's items. */
export type ItemSuggestions = {
	item: string;
	title: string;
	/** The latest version, against which questions are numbered. */
	version: number;
	questions: QuestionSuggestions[];
};

/** The answers the author accepts for one question. */
export type Accepted = { question: number; answers: string[] };

/** A new version as the share route takes it. */
export type Prepared = Record<string, unknown> & { item: string; title: string };

export function suggestions(fetch: typeof globalThis.fetch, root: string = LIBRARY_API) {
	const call = caller(fetch, root, 'suggestions');

	return {
		/** The suggestions of the session's items that have any. */
		async received(session: string) {
			return (
				await call<{ items: ItemSuggestions[] }>('GET', '/api/account/suggestions', { session })
			).items;
		},
		/**
		 * Accepts `accepted` into a new version of `item` and shares it: the
		 * share route reviews it, and answers its outcome.
		 */
		async accept(session: string, item: string, accepted: Accepted[]) {
			const { prepared } = await call<{ prepared: Prepared }>(
				'POST',
				`/api/account/items/${encodeURIComponent(item)}/suggestions/accept`,
				{ session, body: { accepted } }
			);
			return call<Share>('POST', '/api/shares', { session, body: prepared });
		}
	};
}
