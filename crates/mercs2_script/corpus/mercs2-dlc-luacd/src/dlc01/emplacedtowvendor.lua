local L0_1, L1_1, L2_1
import("MrxPmc", false)
import("MrxUtil", false)
import("MrxSubtitle", false)
L0_1 = tEvents
if not L0_1 then
  L0_1 = {}
end
tEvents = L0_1
L0_1 = tMarker
if not L0_1 then
  L0_1 = {}
end
tMarker = L0_1
nCost = 500000

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L2_2 = tEvents
  L3_2 = tEvents[A0_2]
  if not L3_2 then
    L3_2 = {}
  end
  L2_2[A0_2] = L3_2
  L2_2 = tMarker
  L3_2 = tMarker[A0_2]
  if not L3_2 then
    L3_2 = {}
  end
  L2_2[A0_2] = L3_2
  L2_2 = tMarker[A0_2]
  L2_2.uMarker = Marker.AddDisc(A0_2, 6, 0, 255, 0, 0.05)
  L2_2 = Event.Create
  L3_2 = Event.ObjectHibernation
  L4_2 = {}
  L4_2[1] = A0_2
  L4_2[2] = "awake"
  
  function L5_2()
    local L0_3, L1_3
    SetupActivationEvents(A0_2)
  end
  
  L2_2(L3_2, L4_2, L5_2)
end

OnActivate = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  OnDeactivate(A0_2)
end

OnDeath = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  L1_2 = tEvents
  if not L1_2 then
    L1_2 = {}
  end
  tEvents = L1_2
  L1_2 = tEvents
  L2_2 = tEvents[A0_2]
  if not L2_2 then
    L2_2 = {}
  end
  L1_2[A0_2] = L2_2
  L1_2 = tMarker[A0_2].uMarker
  if L1_2 then
    Marker.Remove(tMarker[A0_2].uMarker)
  end
  L1_2 = tEvents[A0_2].uCheckEvent
  if L1_2 then
    Event.Delete(tEvents[A0_2].uCheckEvent)
    L1_2 = tEvents[A0_2]
    L1_2.uCheckEvent = nil
  end
  L1_2 = tEvents[A0_2].uActivate
  if L1_2 then
    Event.Delete(tEvents[A0_2].uActivate)
    L1_2 = tEvents[A0_2]
    L1_2.uActivate = nil
  end
  L1_2 = tEvents[A0_2].uDeactivate
  if L1_2 then
    Event.Delete(tEvents[A0_2].uDeactivate)
    L1_2 = tEvents[A0_2]
    L1_2.uDeactivate = nil
  end
  L1_2 = tEvents
  L1_2[A0_2] = nil
end

OnDeactivate = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  Pg.AddContextAction(A0_2, ("Purchase Emplaced TOW $" .. MrxUtil.FormatMoney(nCost)), 6)
  L1_2 = tEvents[A0_2]
  L4_2 = {}
  L4_2[1] = Player.GetAnyCharacter()
  L4_2[2] = A0_2
  L6_2 = {}
  L6_2[1] = A0_2
  L1_2.uActivate = Event.CreatePersistent(Event.ContextAction, L4_2, PurchaseEmplacement, L6_2)
end

SetupActivationEvents = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2
  L1_2 = Object.GetPosition
  L2_2 = A0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  L4_2 = Object.GetYaw(A0_2)
  L5_2 = MrxPmc.GetCashQty()
  L7_2 = table.getn(Pg.GetAwakeObjects(L1_2, L2_2, L3_2, 4, "Emplacedweapon"))
  if L7_2 < 1 then
    L8_2 = nCost
    if L5_2 > L8_2 then
      MrxPmc.AddCashQty(-nCost)
      L8_2 = Pg.Spawn("Emplaced TOW (Allied) (seatbelt)", L1_2, (L2_2 + 3.25), L3_2, L4_2, false, true)
      L10_2 = Vehicle.GetRiders(L8_2)[1]
      L11_2 = Pg.FastCollectHumans(L1_2, L2_2, L3_2, 45, "China && Vehicle")
      L13_2 = {}
      L13_2.AIGuid = L10_2
      L13_2.AnchorRadius = 2
      L13_2.AnchorPosition = L1_2
      L13_2[1] = L2_2
      L13_2[2] = L3_2
      Ai.Anchor(L13_2)
      L13_2 = {}
      L13_2.AIGuid = L10_2
      L13_2.Goal = "Attack"
      L13_2.Target = L11_2[1]
      L13_2.Priority = "medPri"
      Ai.Goal(L13_2)
      MrxSubtitle.Add("[green]Emplacement Purchased!!!")
      GunnerLeaves(A0_2, L8_2, L10_2)
    else
      MrxSubtitle.Add("[red]Insufficient Funds!!!")
    end
  else
    MrxSubtitle.Add("[red]Already occupied!!!")
  end
end

PurchaseEmplacement = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L3_2 = Event.Create
  L4_2 = Event.ObjectInSeat
  L5_2 = {}
  L5_2[1] = A2_2
  L5_2[2] = A1_2
  L5_2[3] = "a"
  L5_2[4] = "x"
  
  function L6_2(A0_3, A1_3)
    local L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3, L9_3
    L4_3 = {}
    L4_3[1] = 10
    L6_3 = {}
    L6_3[1] = A0_3
    L6_3[2] = 1
    L6_3[3] = true
    eFadeOut = Event.Create(Event.TimerRelative, L4_3, Object.FadeOut, L6_3)
  end
  
  L7_2 = {}
  L7_2[1] = A1_2
  L7_2[2] = A2_2
  eGunnerLeaves = L3_2(L4_2, L5_2, L6_2, L7_2)
  L3_2 = Event.Create
  L4_2 = Event.ObjectInSeat
  L5_2 = {}
  L5_2[1] = A2_2
  L5_2[2] = A1_2
  L5_2[3] = "a"
  L5_2[4] = "e"
  
  function L6_2()
    local L0_3, L1_3, L2_3, L3_3
    Event.Delete(eFadeOut)
    GunnerLeaves(A0_2, A1_2, A2_2)
  end
  
  eGunnerEnters = L3_2(L4_2, L5_2, L6_2, {})
end

GunnerLeaves = L0_1
