<script lang="ts">
	import type { Path } from '$app/types';
	import { resolve } from '$app/paths';
	import { getLocale, localizeHref } from '#lib/paraglide/runtime.js';
	import { m } from '#lib/paraglide/messages.js';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import type { Source } from '#lib/server/items.ts';
	import { REPORT_REASONS } from '#lib/report-reasons.ts';
	import { reasonWord } from '#lib/moderation-words.ts';
	import type { PageProps } from './$types';

	let { data, form }: PageProps = $props();
	const item = $derived(data.item);

	const refusals: Record<string, () => string> = {
		signed_out: m.refused_signed_out,
		no_account: m.refused_no_account,
		card_required: m.refused_card_required,
		bad_comment: m.refused_bad_comment,
		comment_quota_reached: m.refused_comment_quota_reached,
		reason_required: m.refused_reason_required,
		unknown_comment: m.refused_unknown_comment
	};
	const code = $derived(form && 'refused' in form ? form.refused : null);
	const refused = $derived(code ? (refusals[code] ?? m.refused_other)() : null);
	/** The comment a report form was sent for, if any. */
	const reportedOn = $derived(
		form && 'reported' in form ? form.reported : form && 'comment' in form ? form.comment : null
	);
	const commented = $derived(form && 'commented' in form ? form.commented : null);
	const written = $derived(form && 'text' in form ? form.text : '');
	const href = (path: string) => resolve(localizeHref(path) as Path);

	const languageNames = $derived(new Intl.DisplayNames([getLocale()], { type: 'language' }));
	const language = (tag: string) => languageNames.of(tag) ?? tag;
	const dates = $derived(
		new Intl.DateTimeFormat(getLocale(), { dateStyle: 'long', timeZone: 'UTC' })
	);
	const date = (instant: string) => dates.format(new Date(instant));

	/** How many families rated it, and their average from 1 to 5 stars. */
	const rated = $derived.by(() => {
		const count = data.ratings.ratings.reduce((sum, n) => sum + n, 0);
		const stars = data.ratings.ratings.reduce((sum, n, i) => sum + n * (i + 1), 0);
		const average = count
			? new Intl.NumberFormat(getLocale(), { maximumFractionDigits: 1 }).format(stars / count)
			: '';
		return { count, average };
	});

	const page = (id: string) => resolve(localizeHref(`/items/${id}`) as Path);
	const file = (version: number) =>
		resolve(localizeHref(`/items/${item.item}/versions/${version}/file`) as Path);
	const list = (values: string[]) =>
		new Intl.ListFormat(getLocale(), { type: 'unit' }).format(values);
	const source = (source: Source) => (typeof source === 'string' ? { title: source } : source);
</script>

<svelte:head>
	<title>{item.title} · {m.site_name()}</title>
	<meta name="description" content={item.metadata.description} />
</svelte:head>

<p class="mb-4 text-sm">
	<a href={resolve(localizeHref('/') as Path)} class="underline-offset-4 hover:underline">
		{m.item_back()}
	</a>
</p>

<article class="grid gap-8">
	<header class="grid gap-3" lang={item.metadata.language}>
		<h1 class="text-2xl font-semibold">{item.title}</h1>
		<p class="text-muted-foreground" lang={getLocale()}>
			{item.type === 'bank' ? m.type_bank() : m.type_path()} ·
			{#if item.profile}
				<a
					href={href(`/authors/${encodeURIComponent(item.profile)}`)}
					class="underline underline-offset-4"
				>
					{m.by_author({ pseudonym: item.author.pseudonym })}
				</a>
			{:else}
				{m.by_author({ pseudonym: item.author.pseudonym })}
			{/if}
		</p>
		<p>{item.metadata.description}</p>
		<div class="flex flex-wrap gap-2" lang={getLocale()}>
			<Badge variant="secondary">{language(item.metadata.language)}</Badge>
			<Badge variant="secondary">{item.metadata.subject}</Badge>
			<Badge variant="secondary">
				{m.ages({ min: item.metadata.ages.min, max: item.metadata.ages.max })}
			</Badge>
			{#if item.metadata.curriculum}
				<Badge variant="outline">
					{item.metadata.curriculum.country} · {item.metadata.curriculum.year} ·
					{item.metadata.curriculum.topic}
				</Badge>
			{/if}
			{#if item.selection}<Badge>{m.selection()}</Badge>{/if}
			{#if item.author.editorial}<Badge variant="outline">{m.editorial()}</Badge>{/if}
			{#if item.editorial?.ai_generated}
				<Badge variant="outline">{m.item_ai_generated()}</Badge>
			{/if}
		</div>
		<div class="grid gap-1 text-sm text-muted-foreground" lang={getLocale()}>
			{#if data.original}
				<p>
					{m.derived_from({ pseudonym: data.original.pseudonym })}
					{#if data.original.title !== null}
						·
						<a href={page(data.original.item)} class="underline underline-offset-4">
							{m.item_original({ title: data.original.title, version: data.original.version })}
						</a>
					{/if}
				</p>
			{/if}
			{#if item.editorial}
				<p>
					{item.editorial.read.by === 'moderator'
						? m.item_read_by_moderator({
								handle: item.editorial.read.handle,
								date: date(item.editorial.read.at)
							})
						: m.item_read_by_operator({ date: date(item.editorial.read.at) })}
				</p>
			{/if}
			{#if item.signals.families > 0}
				<p>{m.item_families({ count: item.signals.families })}</p>
			{/if}
			<p>
				{m.licence()} ·
				<a
					href="https://creativecommons.org/licenses/by-sa/4.0/"
					rel="license external"
					class="underline underline-offset-4"
				>
					{m.item_licence_link()}
				</a>
			</p>
		</div>
	</header>

	<section class="grid gap-2 rounded-lg border p-4" aria-labelledby="download">
		<h2 id="download" class="sr-only">{m.item_download({ version: item.version })}</h2>
		<div>
			<Button href={file(item.version)} download data-sveltekit-reload>
				{m.item_download({ version: item.version })}
			</Button>
		</div>
		<p class="text-sm text-muted-foreground">{m.item_download_help()}</p>
	</section>

	{#if item.content.questions?.length}
		<section class="grid gap-3" lang={item.metadata.language}>
			<h2 class="text-xl font-semibold" lang={getLocale()}>{m.item_questions()}</h2>
			<ol class="grid list-decimal gap-3 pl-6">
				{#each item.content.questions as question, i (i)}
					<li>
						<p>{question.prompt}</p>
						{#if question.choices?.length}
							<p class="text-sm text-muted-foreground">
								{m.item_choices({ choices: list(question.choices) })}
							</p>
						{/if}
						{#if question.answers?.length}
							<p class="text-sm text-muted-foreground">
								{m.item_answers({ answers: list(question.answers) })}
							</p>
						{/if}
						{#if question.explanation}
							<p class="text-sm">{question.explanation}</p>
						{/if}
					</li>
				{/each}
			</ol>
		</section>
	{/if}

	{#if item.content.lessons?.length}
		<section class="grid gap-3" lang={item.metadata.language}>
			<h2 class="text-xl font-semibold" lang={getLocale()}>{m.item_lessons()}</h2>
			{#each item.content.lessons as lesson, i (i)}
				<div>
					{#if lesson.title}<h3 class="font-medium">{lesson.title}</h3>{/if}
					<p class="whitespace-pre-line">{lesson.text}</p>
				</div>
			{/each}
		</section>
	{/if}

	{#if item.content.sources?.length}
		<section class="grid gap-2">
			<h2 class="text-xl font-semibold">{m.item_sources()}</h2>
			<ul class="list-disc pl-6">
				{#each item.content.sources.map(source) as { title, url }, i (i)}
					<li>
						{#if url}
							<a href={url} rel="external nofollow" class="underline underline-offset-4">{title}</a>
						{:else}{title}{/if}
					</li>
				{/each}
			</ul>
		</section>
	{/if}

	<section class="grid gap-2">
		<h2 class="text-xl font-semibold">{m.item_ratings()}</h2>
		{#if rated.count === 0}
			<p class="text-muted-foreground">{m.item_ratings_none()}</p>
		{:else}
			<p>{m.item_ratings_summary({ average: rated.average, count: rated.count })}</p>
		{/if}
		{#if data.ratings.reviews.length}
			<ul class="grid gap-2">
				{#each data.ratings.reviews as review, i (i)}
					<li class="rounded-lg border p-3">
						<p class="text-sm text-muted-foreground">{m.item_stars({ stars: review.stars })}</p>
						<p lang={item.metadata.language}>{review.text}</p>
					</li>
				{/each}
			</ul>
		{/if}
	</section>

	<section class="grid gap-3" aria-labelledby="comments">
		<h2 id="comments" class="text-xl font-semibold">{m.item_comments()}</h2>
		{#if item.comments?.length}
			<ul class="grid gap-2">
				{#each item.comments as comment (comment.comment)}
					<li class="grid gap-1 rounded-lg border p-3">
						<p class="text-sm text-muted-foreground">
							{comment.pseudonym} · {date(comment.published_at)}
						</p>
						<p class="whitespace-pre-line" lang={item.metadata.language}>{comment.text}</p>
						{#if reportedOn === comment.comment && form && 'reported' in form}
							<p role="status" class="text-sm">{m.reported_comment()}</p>
						{:else}
							<details class="text-sm">
								<summary class="cursor-pointer text-muted-foreground">{m.report_comment()}</summary>
								<form method="post" action="?/report" class="mt-2 grid gap-2">
									<input type="hidden" name="comment" value={comment.comment} />
									<label class="grid gap-1">
										{m.report_reason()}
										<select
											name="reason"
											required
											class="rounded-md border bg-transparent px-3 py-2"
										>
											<option value=""></option>
											{#each REPORT_REASONS as reason (reason)}
												<option value={reason}>{reasonWord(reason)}</option>
											{/each}
										</select>
									</label>
									<label class="grid gap-1">
										{m.report_details()}
										<textarea
											name="details"
											rows="2"
											maxlength="2000"
											class="rounded-md border bg-transparent px-3 py-2"></textarea>
									</label>
									<div>
										<Button type="submit" variant="outline" size="sm">{m.report_submit()}</Button>
									</div>
								</form>
								{#if reportedOn === comment.comment && refused}
									<p role="alert" class="text-destructive">{refused}</p>
								{/if}
							</details>
						{/if}
					</li>
				{/each}
			</ul>
		{:else}
			<p class="text-muted-foreground">{m.comment_none()}</p>
		{/if}

		{#if commented === 'published'}
			<p role="status">{m.comment_published()}</p>
		{:else if commented === 'held'}
			<p role="status">{m.comment_held()}</p>
		{:else if commented === 'refused' && form && 'reasons' in form}
			<div role="alert">
				<p>{m.comment_refused()}</p>
				<ul class="mt-1 grid list-disc gap-1 pl-5">
					{#each form.reasons as reason, n (n)}
						<li>{reasonWord(reason.reason)}: {reason.explanation}</li>
					{/each}
				</ul>
			</div>
		{/if}
		{#if refused && !reportedOn}
			<p role="alert" class="text-destructive">
				{refused}
				{#if code === 'no_account' || code === 'signed_out'}
					<a href={href('/account')} class="underline">{m.account_link()}</a>
				{:else if code === 'card_required'}
					<a href={href('/account/card')} class="underline">{m.card_link()}</a>
				{/if}
			</p>
		{/if}

		{#if data.signedIn}
			<form method="post" action="?/comment" class="grid gap-2">
				<label class="grid gap-1">
					{m.comment_label()}
					<textarea
						name="text"
						required
						rows="3"
						maxlength="1000"
						class="rounded-md border bg-transparent px-3 py-2"
						value={commented === 'published' || commented === 'held' ? '' : written}></textarea>
				</label>
				<p class="text-sm text-muted-foreground">{m.comment_help()}</p>
				<div>
					<Button type="submit">{m.comment_submit()}</Button>
				</div>
			</form>
		{:else}
			<p class="text-sm">
				<a href={href('/account')} class="underline">{m.comment_sign_in()}</a>
			</p>
		{/if}
	</section>

	<section class="grid gap-2">
		<h2 class="text-xl font-semibold">{m.item_versions()}</h2>
		<ul class="grid gap-1">
			{#each item.versions as { version, published_at } (version)}
				<li class="flex flex-wrap items-center gap-x-3">
					<span>{m.item_version({ version, date: date(published_at) })}</span>
					<!-- The latest version is downloaded from the button above. -->
					{#if version !== item.version}<a
							href={file(version)}
							download
							data-sveltekit-reload
							class="text-sm underline underline-offset-4"
						>
							{m.item_download({ version })}
						</a>{/if}
				</li>
			{/each}
		</ul>
	</section>
</article>
