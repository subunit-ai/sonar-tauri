import { Canvas, useFrame } from "@react-three/fiber";
import { useRef } from "react";
import type { Group, Mesh } from "three";

// Prozeduraler Unit-One-Kristall fürs Forge-„arbeitet"-Overlay (R3F v9, React-19-kompatibel).
// Bewusst asset-frei (kein 9 MB-GLB im Installer): facettierte Cyan-Glas-Hülle + emissiver Kern,
// dreht sich dauerhaft → signalisiert „Forge arbeitet gerade an deinem Gerät". Look an den
// call.subunit.ai-Kristall angelehnt (Cyan #00ffee, faceted glass).
function Crystal() {
  const group = useRef<Group>(null);
  const core = useRef<Mesh>(null);

  useFrame((state, delta) => {
    const d = Math.min(0.05, delta); // gegen Sprünge bei Tab-Wechsel
    if (group.current) {
      group.current.rotation.y += d * 0.6;
      group.current.rotation.x += d * 0.15;
    }
    // sanftes Atmen des Kerns
    const t = state.clock.elapsedTime;
    const s = 1 + Math.sin(t * 2) * 0.06;
    if (core.current) core.current.scale.setScalar(s);
  });

  return (
    <group ref={group}>
      {/* Äußere facettierte Glas-Hülle */}
      <mesh>
        <icosahedronGeometry args={[1.3, 0]} />
        <meshPhysicalMaterial
          color="#0891b2"
          roughness={0.08}
          metalness={0.1}
          transmission={0.85}
          thickness={1.2}
          transparent
          opacity={0.55}
          ior={1.4}
        />
      </mesh>
      {/* Innerer emissiver Kern */}
      <mesh ref={core}>
        <icosahedronGeometry args={[0.7, 0]} />
        <meshStandardMaterial
          color="#00ffee"
          emissive="#00ffee"
          emissiveIntensity={2.2}
          toneMapped={false}
        />
      </mesh>
    </group>
  );
}

export function CrystalOverlay() {
  return (
    <Canvas
      style={{ width: "100%", height: "100%", background: "transparent" }}
      gl={{ alpha: true, antialias: true }}
      camera={{ position: [0, 0, 4], fov: 45 }}
    >
      <ambientLight intensity={0.4} />
      <pointLight position={[3, 3, 3]} intensity={2} color="#00ffee" />
      <pointLight position={[-3, -2, 2]} intensity={1} color="#0891b2" />
      <Crystal />
    </Canvas>
  );
}
