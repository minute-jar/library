import { error } from '@sveltejs/kit';
import { NotFound, items } from '#lib/server/items.ts';
import type { PageServerLoad } from './$types';

export const load = (async ({ params, fetch }) => {
	const api = items(fetch);
	try {
		const [item, ratings] = await Promise.all([api.page(params.item), api.ratings(params.item)]);
		// A derived work links to its original by title while the catalogue
		// shows it; its attribution stays whatever became of it.
		const origin = item.derived_from;
		const original = origin && {
			...origin,
			title: await api.page(origin.item).then(
				(page) => page.title,
				() => null
			)
		};
		return { item, ratings, original };
	} catch (failure) {
		if (failure instanceof NotFound) error(404, 'No such shared item');
		error(503, 'The catalogue is unavailable');
	}
}) satisfies PageServerLoad;
