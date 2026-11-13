-- One half of the gate's cycle: imports qm_cyc_b, which imports this module.
import("qm_cyc_b")
print("[qmcyc] a body start: b=" .. tostring(qm_cyc_b) .. " b.ran=" .. tostring(qm_cyc_b and qm_cyc_b.ran))
ran = true
print("[qmcyc] a body end")

function Init()
    print("[qmcyc] a Init: b.ran=" .. tostring(qm_cyc_b and qm_cyc_b.ran))
end
