import { error } from '@sveltejs/kit';
import { BadFilter, catalogue, chosen } from '#lib/server/catalogue.ts';
import type { PageServerLoad } from './$types';

export const load = (async ({ url, fetch }) => {
	const api = catalogue(fetch);
	const query = chosen(url.searchParams);
	try {
		const [found, filters] = await Promise.all([
			api.browse(query).catch((failure: unknown) => {
				if (failure instanceof BadFilter) return null;
				throw failure;
			}),
			api.filters()
		]);
		return { query, found, badFilter: found === null, filters };
	} catch {
		error(503, 'The catalogue is unavailable');
	}
}) satisfies PageServerLoad;
