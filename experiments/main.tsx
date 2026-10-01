import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { motion, MotionConfig } from "motion/react";
import gsap from "gsap";
import * as THREE from "three";
import { MeshGradient } from "@paper-design/shaders-react";

function SpatialTrial() {
  const ref = useRef<HTMLDivElement>(null);
  const [fallback, setFallback] = useState(false);
  useEffect(() => {
    let renderer: THREE.WebGLRenderer;
    try {
      renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    } catch {
      setFallback(true);
      return;
    }
    renderer.setSize(380, 160);
    renderer.setPixelRatio(Math.min(devicePixelRatio, 1.5));
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(35, 380 / 160, 0.1, 100);
    camera.position.set(5, 4, 7);
    camera.lookAt(0, 1, 0);
    const objects: THREE.Mesh[] = [];
    [1.8, 1.1, 2.3, 0.7].forEach((height, i) => {
      const mesh = new THREE.Mesh(
        new THREE.BoxGeometry(0.6, height, 0.6),
        new THREE.MeshNormalMaterial(),
      );
      mesh.position.set(i - 1.5, height / 2, 0);
      scene.add(mesh);
      objects.push(mesh);
    });
    const host = ref.current!;
    host.appendChild(renderer.domElement);
    renderer.render(scene, camera);
    const pointer = (event: PointerEvent) => {
      if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
      scene.rotation.y = (event.offsetX / 380 - 0.5) * 0.7;
      renderer.render(scene, camera);
    };
    host.addEventListener("pointermove", pointer);
    return () => {
      host.removeEventListener("pointermove", pointer);
      objects.forEach((o) => {
        o.geometry.dispose();
        (o.material as THREE.Material).dispose();
      });
      renderer.dispose();
      renderer.domElement.remove();
    };
  }, []);
  return (
    <div ref={ref}>
      {fallback && (
        <p>
          WebGL unavailable. File type totals remain readable as a standard
          list.
        </p>
      )}
    </div>
  );
}
function App() {
  const ref = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState(false);
  useEffect(() => {
    const mm = gsap.matchMedia();
    mm.add("(prefers-reduced-motion: no-preference)", () => {
      gsap.to(ref.current, {
        x: 280,
        duration: 1.6,
        repeat: -1,
        yoyo: true,
        ease: "power2.inOut",
      });
    });
    return () => mm.revert();
  }, []);
  return (
    <main
      style={{
        font: "14px Segoe UI,sans-serif",
        color: "#24382e",
        padding: 35,
      }}
    >
      <h1>Interaction trials</h1>
      <p>Development prototypes · synthetic values · no filesystem access</p>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(2, minmax(0,1fr))",
          gap: 30,
          maxWidth: 1000,
        }}
      >
        <section>
          <h2>GSAP · transfer indicator</h2>
          <p>A packet travels between two endpoints while a scan runs.</p>
          <div style={{ height: 100, background: "#edf3ef", padding: 30 }}>
            <div
              ref={ref}
              style={{
                width: 35,
                height: 20,
                background: "#176b51",
                borderRadius: 4,
              }}
            />
          </div>
        </section>
        <section>
          <h2>Motion · selection feedback</h2>
          <p>Action controls appear when the first file is selected.</p>
          <button onClick={() => setSelected(!selected)}>
            Toggle selection
          </button>
          <motion.div
            animate={{ y: selected ? 0 : 8, opacity: selected ? 1 : 0.3 }}
            style={{
              background: "#243b32",
              color: "white",
              padding: 18,
              marginTop: 10,
            }}
          >
            1 file selected · Review action
          </motion.div>
        </section>
        <section>
          <h2>Three.js · storage distribution</h2>
          <p>Pointer movement turns a small spatial file-type chart.</p>
          <SpatialTrial />
        </section>
        <section>
          <h2>Paper Shaders · active scan surface</h2>
          <p>A quiet activity texture, with a static fallback surface.</p>
          <div style={{ background: "#edf3ef", height: 160 }}>
            <MeshGradient
              style={{ height: 160, width: "100%" }}
              colors={["#176b51", "#d3e4d8", "#f1f7f2", "#81aa93"]}
              speed={
                matchMedia("(prefers-reduced-motion: reduce)").matches
                  ? 0
                  : 0.12
              }
            />
          </div>
        </section>
      </div>
    </main>
  );
}
createRoot(document.getElementById("root")!).render(
  <MotionConfig reducedMotion="user">
    <App />
  </MotionConfig>,
);
