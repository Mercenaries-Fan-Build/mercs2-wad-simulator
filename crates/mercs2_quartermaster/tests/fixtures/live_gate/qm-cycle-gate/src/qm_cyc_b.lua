-- The other half of the gate's cycle: imports qm_cyc_a, which imports this module.
import("qm_cyc_a")
print("[qmcyc] b body start: a=" .. tostring(qm_cyc_a) .. " a.ran=" .. tostring(qm_cyc_a and qm_cyc_a.ran))
ran = true
print("[qmcyc] b body end")

function Init()
    print("[qmcyc] b Init: a.ran=" .. tostring(qm_cyc_a and qm_cyc_a.ran))
end
