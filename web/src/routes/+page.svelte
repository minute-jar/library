<script lang="ts">
	import type { Path } from '$app/types';
	import { resolve } from '$app/paths';
	import { getLocale, localizeHref } from '#lib/paraglide/runtime.js';
	import { m } from '#lib/paraglide/messages.js';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import { Input } from '#lib/components/ui/input/index.js';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const languageNames = $derived(new Intl.DisplayNames([getLocale()], { type: 'language' }));
	const language = (tag: string) => languageNames.of(tag) ?? tag;
	const unique = (values: string[]) => [...new Set(values)].sort();
	const curriculum = $derived([
		['country', m.filter_country(), unique(data.filters.curricula.map((c) => c.country))],
		['year', m.filter_year(), unique(data.filters.curricula.map((c) => c.year))],
		['topic', m.filter_topic(), unique(data.filters.curricula.map((c) => c.topic))]
	] as const);

	/** The query string of this search at another page. */
	function atPage(page: number) {
		const params = { ...data.query, page: String(page) };
		return `?${Object.entries(params)
			.map(([name, value]) => `${name}=${encodeURIComponent(value)}`)
			.join('&')}`;
	}

	const select =
		'h-8 w-full rounded-lg border border-input bg-transparent px-2 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30';
</script>

<h1 class="mb-1 text-2xl font-semibold">{m.catalogue_title()}</h1>
<p class="mb-6 text-muted-foreground">{m.site_tagline()}</p>

<form method="get" class="mb-6 grid gap-4" role="search">
	<div class="flex gap-2">
		<label for="q" class="sr-only">{m.search_label()}</label>
		<Input
			id="q"
			name="q"
			type="search"
			value={data.query.q ?? ''}
			placeholder={m.search_placeholder()}
		/>
		<Button type="submit">{m.search_submit()}</Button>
	</div>
	<fieldset class="grid gap-3 sm:grid-cols-3 lg:grid-cols-6">
		<label class="grid gap-1 text-sm">
			{m.filter_language()}
			<select name="language" class={select} value={data.query.language ?? ''}>
				<option value="">{m.filter_any()}</option>
				{#each data.filters.languages as tag (tag)}
					<option value={tag}>{language(tag)}</option>
				{/each}
			</select>
		</label>
		<label class="grid gap-1 text-sm">
			{m.filter_subject()}
			<select name="subject" class={select} value={data.query.subject ?? ''}>
				<option value="">{m.filter_any()}</option>
				{#each data.filters.subjects as subject (subject)}
					<option value={subject}>{subject}</option>
				{/each}
			</select>
		</label>
		<label class="grid gap-1 text-sm">
			{m.filter_age()}
			<Input name="age" type="number" min="0" max="25" value={data.query.age ?? ''} />
		</label>
		{#each curriculum as [name, label, values] (name)}
			<label class="grid gap-1 text-sm">
				{label}
				<select {name} class={select} value={data.query[name] ?? ''}>
					<option value="">{m.filter_any()}</option>
					{#each values as value (value)}
						<option {value}>{value}</option>
					{/each}
				</select>
			</label>
		{/each}
	</fieldset>
	<div>
		<Button variant="link" href="?" class="px-0">{m.filter_clear()}</Button>
	</div>
</form>

{#if data.badFilter}
	<p role="alert" class="text-destructive">{m.bad_filter()}</p>
{:else if data.found}
	{#if data.found.total === 0}
		<p>{m.found_none()}</p>
	{:else}
		<p class="mb-3 text-sm text-muted-foreground">
			{m.found_count({ count: data.found.total })}
		</p>
		<ul class="grid gap-4 sm:grid-cols-2">
			{#each data.found.items as entry (entry.item)}
				<li>
					<Card.Root lang={entry.metadata.language} class="h-full">
						<Card.Header>
							<Card.Title>
								<h2>
									<a
										href={resolve(localizeHref(`/items/${entry.item}`) as Path)}
										class="underline-offset-4 hover:underline"
									>
										{entry.title}
									</a>
								</h2>
							</Card.Title>
							<Card.Description lang={getLocale()}>
								{entry.type === 'bank' ? m.type_bank() : m.type_path()} ·
								{m.by_author({ pseudonym: entry.author.pseudonym })}
							</Card.Description>
						</Card.Header>
						<Card.Content class="grid gap-3">
							<p>{entry.metadata.description}</p>
							<div class="flex flex-wrap gap-2" lang={getLocale()}>
								<Badge variant="secondary">{language(entry.metadata.language)}</Badge>
								<Badge variant="secondary">{entry.metadata.subject}</Badge>
								<Badge variant="secondary">
									{m.ages({ min: entry.metadata.ages.min, max: entry.metadata.ages.max })}
								</Badge>
								{#if entry.metadata.curriculum}
									<Badge variant="outline">
										{entry.metadata.curriculum.country} · {entry.metadata.curriculum.year} ·
										{entry.metadata.curriculum.topic}
									</Badge>
								{/if}
								{#if entry.selection}<Badge>{m.selection()}</Badge>{/if}
								{#if entry.author.editorial}<Badge variant="outline">{m.editorial()}</Badge>{/if}
							</div>
							{#if entry.derived_from}
								<p class="text-sm text-muted-foreground" lang={getLocale()}>
									<a
										href={resolve(localizeHref(`/items/${entry.derived_from.item}`) as Path)}
										class="underline-offset-4 hover:underline"
									>
										{m.derived_from({ pseudonym: entry.derived_from.pseudonym })}
									</a>
								</p>
							{/if}
							<p class="text-xs text-muted-foreground" lang={getLocale()}>{m.licence()}</p>
						</Card.Content>
					</Card.Root>
				</li>
			{/each}
		</ul>
		{#if data.found.pages > 1}
			<nav class="mt-6 flex items-center justify-between text-sm" aria-label={m.pagination()}>
				{#if data.found.page > 1}
					<Button variant="outline" href={atPage(data.found.page - 1)} rel="prev">
						{m.page_previous()}
					</Button>
				{:else}<span></span>{/if}
				<span>{m.page_of({ page: data.found.page, pages: data.found.pages })}</span>
				{#if data.found.page < data.found.pages}
					<Button variant="outline" href={atPage(data.found.page + 1)} rel="next">
						{m.page_next()}
					</Button>
				{:else}<span></span>{/if}
			</nav>
		{/if}
	{/if}
{/if}
