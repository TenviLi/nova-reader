import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";
import type { Snippet } from "svelte";
import type { HTMLAttributes } from "svelte/elements";

import { getLocale } from '$lib/paraglide/runtime.js';

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}

/**
 * Extract a human-readable error message from any thrown value.
 * PLT: Closed error algebra — all error shapes reduced to a single string output.
 * ApiRequestError extends Error, so instanceof Error catches it.
 */
export function getErrorMessage(error: unknown): string {
	if (error instanceof Error) return error.message;
	if (typeof error === 'string') return error;
	return '操作失败，请重试';
}

export function formatLocaleNumber(value: number): string {
	return new Intl.NumberFormat(getLocale()).format(value);
}

export type WithElementRef<T = HTMLAttributes<HTMLElement>> = T & {
	ref?: HTMLElement | null;
};

export type WithoutChild<T> = Omit<T, "child">;
export type WithoutChildren<T> = Omit<T, "child" | "children">;

export type WithoutChildrenOrChild<T> = Omit<T, "children" | "child">;
