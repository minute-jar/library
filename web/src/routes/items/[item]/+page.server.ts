import { error } from '@sveltejs/kit';
import { NotFound, items } from '#lib/server/items.ts';
import type { PageServerLoad } from './$types';

export const load = (async ({ params, fetch }) => {
	const api = items(fetch);
	try {
		const [item, ratings] = await Promise.all([api.page(params.item), api.ratings(params.item)]);
		return { item, ratings };
	} catch (failure) {
		if (failure instanceof NotFound) error(404, 'No such shared item');
		error(503, 'The catalogue is unavailable');
	}
}) satisfies PageServerLoad;
