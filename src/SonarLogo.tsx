/** Sonar-Logo — konzentrische Ping-/Radar-Wellen um einen Kern.
 *  `pulsing` lässt einen Ring nach außen laufen (Splash/Login/Overlay). */
export function SonarLogo({
  size = 32,
  pulsing = false,
}: {
  size?: number;
  pulsing?: boolean;
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 48 48"
      fill="none"
      role="img"
      aria-label="Sonar"
    >
      <circle cx="24" cy="24" r="20" stroke="#06b6d4" strokeOpacity="0.22" strokeWidth="2" />
      <circle cx="24" cy="24" r="13.5" stroke="#06b6d4" strokeOpacity="0.5" strokeWidth="2" />
      <circle cx="24" cy="24" r="7.5" stroke="#22d3ee" strokeOpacity="0.85" strokeWidth="2.2" />
      <circle cx="24" cy="24" r="3.4" fill="#06b6d4" />
      {pulsing ? (
        <circle cx="24" cy="24" r="4" fill="none" stroke="#22d3ee" strokeWidth="2">
          <animate attributeName="r" values="4;21" dur="2.2s" repeatCount="indefinite" />
          <animate
            attributeName="stroke-opacity"
            values="0.9;0"
            dur="2.2s"
            repeatCount="indefinite"
          />
        </circle>
      ) : null}
    </svg>
  );
}
