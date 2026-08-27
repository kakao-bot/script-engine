export function roll(sides) {
    return 1 + Math.floor(Math.random() * sides);
}

export function rollMany(count, sides) {
    return Array.from({ length: count }, () => roll(sides));
}
