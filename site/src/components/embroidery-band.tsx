// A folk cross-stitch band — a repeating flower + zigzag motif, in the duckling
// palette. Used as a textured strip along the bottom of the hero.
export function EmbroideryBand({ className }: { className?: string }) {
  // the duckling palette, from the theme tokens in global.css
  const y = "var(--entl-yellow)";
  const yd = "var(--entl-yellow-dark)";
  // one cross-stitch flower: 8 petals around a centre, each a small square
  const stitch = (cx: number, cy: number, fill: string, opacity = 1) => (
    <rect
      key={`${cx}-${cy}`}
      x={cx - 2}
      y={cy - 2}
      width={4}
      height={4}
      fill={fill}
      opacity={opacity}
      rx={0.5}
    />
  );
  return (
    <svg
      className={className}
      width="100%"
      height="40"
      role="presentation"
      aria-hidden="true"
      preserveAspectRatio="xMidYMax slice"
      xmlns="http://www.w3.org/2000/svg"
    >
      <defs>
        <pattern
          id="entl-embroidery"
          patternUnits="userSpaceOnUse"
          width="56"
          height="40"
        >
          {/* cross-stitch flower, centred at (28,15) */}
          {stitch(28, 15, yd)}
          {stitch(28, 8, y)}
          {stitch(28, 22, y)}
          {stitch(21, 15, y)}
          {stitch(35, 15, y)}
          {stitch(33.5, 9.5, y, 0.8)}
          {stitch(22.5, 9.5, y, 0.8)}
          {stitch(33.5, 20.5, y, 0.8)}
          {stitch(22.5, 20.5, y, 0.8)}
          {/* side buds */}
          {stitch(4, 14, y, 0.7)}
          {stitch(52, 14, y, 0.7)}
          {/* zigzag baseline */}
          <polyline
            points="0,33 14,27 28,33 42,27 56,33"
            fill="none"
            stroke={y}
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            opacity="0.85"
          />
          {/* peak dots */}
          <circle cx="14" cy="27" r="1.7" fill={yd} />
          <circle cx="42" cy="27" r="1.7" fill={yd} />
        </pattern>
      </defs>
      <rect width="100%" height="40" fill="url(#entl-embroidery)" />
    </svg>
  );
}
