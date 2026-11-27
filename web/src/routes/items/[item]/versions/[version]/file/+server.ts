import { error } from '@sveltejs/kit';
import { NotFound, items } from '#lib/server/items.ts';
import type { RequestHandler } from './$types';

/** The item file, handed over as the API made it: its bytes are checked by a family server. */
export const GET = (async ({ params, fetch }) => {
	let file: Response;
	try {
		file = await items(fetch).file(params.item, params.version);
	} catch (failure) {
		if (failure instanceof NotFound) error(404, 'No such version');
		error(503, 'The catalogue is unavailable');
	}
	const headers = new Headers({ 'content-type': 'application/json' });
	const disposition = file.headers.get('content-disposition');
	if (disposition) headers.set('content-disposition', disposition);
	return new Response(await file.arrayBuffer(), { headers });
}) satisfies RequestHandler;
