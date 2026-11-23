local L0_1, L1_1, L2_1
import("MrxSupport", false)
sDeliveryVehicle = "UH1 Transport (PMC) (Driver)"
sCargoToDeliver = "box"
uCargoToDeliver = Pg.GetGuidByName(sCargoToDeliver)
nAltitude = 25

function L0_1(A0_2, A1_2, A2_2, A3_2, A4_2, A5_2, A6_2, A7_2, A8_2)
  local L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2
  L9_2 = Net.IsClient()
  if L9_2 then
    return
  end
  L9_2 = Pg.GetGuidByName(A1_2)
  if not L9_2 then
  end
  L10_2 = {}
  L10_2.PMC = "UH1 Transport (PMC) (Driver)"
  L10_2.AL = "MH53J (Driver)"
  L10_2.CH = "Ka29b (Driver)"
  L10_2.GR = "UH1 Transport (GR) (Driver)"
  L10_2.OC = "Coanda Transport (Driver)"
  L10_2.PR = "Alouette3 Transport (PR) (Driver)"
  L10_2.VZ = "Alouette3 Transport (VZ) (Driver)"
  L10_2.VZH = "Mi26 (VZ) (Driver)"
  L10_2.VZHF = "Mi26 (VZA Intro) (Driver)"
  L10_2.VZF = "Alouette3 Transport (VZA Intro) (Driver)"
  L11_2 = Pg.GetGuidByName(L10_2[A0_2])
  if not L11_2 then
    L11_2 = Pg.GetGuidByName("Ka29b (Driver)")
  end
  L12_2 = Object.GetHibernationDistance(L9_2)
  if not L12_2 then
    L12_2 = 200
  end
  L13_2 = Pg.SpawnFromCamera(L9_2, Math.min((L12_2 - 5), 200), 1, true, Player.GetLocalCharacter(), false, true)
  if not L13_2 then
    return
  end
  if not A6_2 then
    L14_2 = Pg.FindPointFromCamera
    L15_2 = L12_2
    L16_2 = nAltitude
    L14_2, L15_2, L16_2 = L14_2(L15_2, L16_2, 10)
    A8_2 = L16_2
    A7_2 = L15_2
    A6_2 = L14_2
    if A3_2 then
      L14_2 = A3_2 + nAltitude
      if A7_2 < L14_2 then
        A7_2 = A3_2 + nAltitude
      end
    end
  end
  L14_2 = Pg.Spawn(L11_2, A6_2, A7_2, A8_2, 0, false, true)
  if not L14_2 then
    Object.Remove(L13_2)
    return
  end
  L17_2 = {}
  L17_2[1] = L14_2
  L17_2[2] = "awake"
  L19_2 = {}
  L19_2[1] = L14_2
  L19_2[2] = L13_2
  L19_2[3] = A2_2
  L19_2[4] = A3_2
  L19_2[5] = A4_2
  L19_2[6] = A5_2
  Event.Create(Event.ObjectHibernation, L17_2, _DeployWinch, L19_2)
  L15_2 = L14_2
  L16_2 = L13_2
  return L15_2, L16_2
end

Create = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2, A4_2, A5_2)
  local L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2
  Object.SetWinchState(A0_2, "deployed")
  L8_2 = {}
  L8_2[1] = 0.1
  L10_2 = {}
  L12_2 = {}
  L12_2[1] = A1_2
  L12_2[2] = "awake"
  L14_2 = {}
  L14_2[1] = A0_2
  L14_2[2] = A1_2
  L14_2[3] = A2_2
  L14_2[4] = A3_2
  L14_2[5] = A4_2
  L14_2[6] = A5_2
  L10_2[1] = Event.ObjectHibernation
  L10_2[2] = L12_2
  L10_2[3] = _WaitCallback
  L10_2[4] = L14_2
  Event.Create(Event.TimerRelative, L8_2, Event.Create, L10_2)
end

_DeployWinch = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2, A4_2, A5_2)
  local L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L8_2 = Object.GetYaw
  L9_2 = A0_2
  L8_2, L9_2, L10_2, L11_2, L12_2 = L8_2(L9_2)
  Object.SetYaw(A1_2, L8_2, L9_2, L10_2, L11_2, L12_2)
  Object.AttachCargoToWinch(A1_2, A0_2)
  Ai.Deliver(Vehicle.GetDriver(A0_2), A2_2, A3_2, A4_2, 0.5, A5_2)
  L8_2 = {}
  L8_2[1] = A1_2
  L8_2[2] = A0_2
  L8_2[3] = "Detach"
  L10_2 = {}
  L10_2[1] = A0_2
  Event.Create(Event.ObjectWinched, L8_2, DeliveryComplete, L10_2)
end

_WaitCallback = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  Object.DetachCargoFromWinch(A0_2)
  self = {}
  MrxSupport.GoHome(self, A0_2)
end

DeliveryComplete = L0_1
