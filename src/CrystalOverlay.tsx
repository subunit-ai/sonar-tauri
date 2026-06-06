import { Suspense, useMemo, useRef, type ComponentProps } from "react";
import { Canvas, useFrame } from "@react-three/fiber";
import { Center, Environment, Float, Lightformer, useGLTF } from "@react-three/drei";
import { EffectComposer, Bloom } from "@react-three/postprocessing";
import * as THREE from "three";

// DER echte UnitOne-Kristall von call.subunit.ai (u1-voice/UnitOneCrystal.jsx), 1:1 portiert
// auf Sonars Stack (R3F v9 / drei v10 / three 0.184). Draco-komprimierte glb (/unitone-crystall.glb)
// + Decoder unter /draco/. Kein Audio hier (Forge-Overlay) → mode idle, ruhiger Atem-Puls.
const GLB_URL = "/unitone-crystall.glb";
const DRACO_PATH = "/draco/";
const CORE_COLOR = new THREE.Color("#00ffee");

function UnitOneCrystal(props: ComponentProps<"group">) {
  const groupRef = useRef<THREE.Group>(null);
  const coreRef = useRef<THREE.Mesh>(null);
  const lightRef = useRef<THREE.PointLight>(null);

  const { nodes } = useGLTF(GLB_URL, DRACO_PATH);
  const geometry = useMemo(() => {
    const mesh = Object.values(nodes).find(
      (n): n is THREE.Mesh => (n as THREE.Mesh)?.isMesh && !!(n as THREE.Mesh).geometry,
    );
    return mesh ? mesh.geometry : null;
  }, [nodes]);

  useFrame((state, delta) => {
    const d = Math.min(1, delta);
    if (groupRef.current) groupRef.current.rotation.y += 0.004; // dreht sich = „u1 arbeitet"
    const t = state.clock.elapsedTime;
    // Ruhiger Atem-Puls (ohne den Audio-Surge wie im Call).
    const targetIntensity = 11 + Math.sin(t * 2.0) * 3;
    if (coreRef.current) {
      const mat = coreRef.current.material as THREE.MeshStandardMaterial;
      mat.emissiveIntensity += (targetIntensity - mat.emissiveIntensity) * d * 6;
    }
    if (lightRef.current) {
      lightRef.current.intensity += (6 - lightRef.current.intensity) * d * 6;
    }
  });

  if (!geometry) return null;
  return (
    <group ref={groupRef} {...props}>
      {/* Innerer emissiver Kern (glüht durch die Schale; toneMapped off → treibt Bloom). */}
      <mesh ref={coreRef} geometry={geometry} scale={0.48} renderOrder={0}>
        <meshStandardMaterial
          color={CORE_COLOR}
          emissive={CORE_COLOR}
          emissiveIntensity={12}
          toneMapped={false}
          side={THREE.FrontSide}
        />
      </mesh>
      {/* Äußere Kristall-Schale — getöntes blaues Glas (Werte 1:1 aus call.subunit.ai). */}
      <mesh geometry={geometry} renderOrder={1}>
        <meshPhysicalMaterial
          color="#112878"
          emissive="#030a22"
          emissiveIntensity={0.3}
          metalness={0.05}
          roughness={0.28}
          clearcoat={0.6}
          clearcoatRoughness={0.25}
          reflectivity={0.85}
          envMapIntensity={2.0}
          iridescence={0.3}
          iridescenceIOR={1.8}
          transparent
          opacity={0.97}
          side={THREE.FrontSide}
          depthWrite={false}
        />
      </mesh>
      <pointLight ref={lightRef} color="#00ddff" intensity={8} distance={2.5} decay={1.2} />
    </group>
  );
}

/** Rotierender UnitOne-Kristall fürs Forge-„u1 arbeitet"-Overlay (echtes call.subunit.ai-Asset). */
export function CrystalOverlay() {
  return (
    <Canvas
      style={{ width: "100%", height: "100%" }}
      camera={{ position: [0, 0, 3.1], fov: 42 }}
      gl={{ antialias: true, alpha: true }}
      dpr={[1, 2]}
    >
      <ambientLight intensity={0.4} />
      <directionalLight position={[3, 4, 5]} intensity={1.2} color="#bfe9ff" />
      <Suspense fallback={null}>
        {/* In-Scene-Env (kein externes HDRI) → Glas-Schale bekommt Reflexionen + Iridescence. */}
        <Environment resolution={256}>
          <Lightformer intensity={2.4} color="#22d3ee" position={[-3, 2, 2]} scale={4} />
          <Lightformer intensity={1.6} color="#6d28d9" position={[3, -2, 2]} scale={4} />
          <Lightformer intensity={1.2} color="#ffffff" position={[0, 3, -3]} scale={3} />
        </Environment>
        {/* Center neutralisiert den off-origin-Pivot der glb; Float = sanftes vertikales Schweben. */}
        <Float speed={2.2} rotationIntensity={0} floatIntensity={1} floatingRange={[-0.08, 0.08]}>
          <Center>
            <UnitOneCrystal scale={2.1} />
          </Center>
        </Float>
      </Suspense>
      <EffectComposer>
        <Bloom mipmapBlur intensity={1.15} luminanceThreshold={0.6} luminanceSmoothing={0.25} radius={0.8} />
      </EffectComposer>
    </Canvas>
  );
}

useGLTF.preload(GLB_URL, DRACO_PATH);
