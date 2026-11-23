local L0_1, L1_1, L2_1
import("MrxUtil", false)

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L1_2 = {}
  L2_2 = {}
  L2_2[1] = "monster truck"
  L2_2[2] = 80
  L2_2[3] = 10
  L3_2 = {}
  L3_2[1] = "escort"
  L3_2[2] = 50
  L3_2[3] = 20
  L4_2 = {}
  L4_2[1] = "Buggy (Hellfire)"
  L4_2[2] = 80
  L4_2[3] = 10
  L1_2[1] = L2_2
  L1_2[2] = L3_2
  L1_2[3] = L4_2
  L3_2 = Pg.SpawnFromCamera(L1_2[A0_2][1], 12)
  L4_2 = Event.Create
  L5_2 = Event.ObjectInSeat
  L6_2 = {}
  L6_2[1] = Player.GetAnyCharacter()
  L6_2[2] = L3_2
  L6_2[3] = "d"
  L6_2[4] = "ei"
  
  function L7_2()
    local L0_3, L1_3, L2_3, L3_3
    gTest = Start(L3_2, L2_2[2], L2_2[3])
  end
  
  L4_2(L5_2, L6_2, L7_2)
end

Test = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L3_2 = {}
  L3_2.uVeh = A0_2
  L5_2 = Object.GetVelocity
  L6_2 = A0_2
  L5_2, L6_2, L7_2, L8_2, L9_2 = L5_2(L6_2)
  L3_2.curSpeed = Math.abs(L5_2, L6_2, L7_2, L8_2, L9_2)
  L3_2.uTimeStamp = Sys.MainTimeStamp()
  L3_2.kInitBombTime = MrxUtil.SetDefault(A2_2, 10)
  L3_2.fMarkedTime = L3_2.kInitBombTime
  L3_2.fTimeLeft = L3_2.kInitBombTime
  L3_2.kSpeedThreshold = ((MrxUtil.SetDefault(A1_2, 80) * 10) / 36)
  L6_2 = {}
  L6_2[1] = 0.1
  L8_2 = {}
  L8_2[1] = L3_2
  L3_2.eTimer = Event.CreatePersistent(Event.TimerRelative, L6_2, Update, L8_2)
  return L3_2
end

Start = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2
  L1_2 = Hud.ObjectiveTray
  L3_2 = {}
  L3_2.vPlayer = nil
  L3_2.nSlot = 2
  L1_2.ClearSlot(L1_2, L3_2)
  L1_2 = Hud.ObjectiveTray
  L3_2 = {}
  L3_2.vPlayer = nil
  L3_2.nSlot = 3
  L1_2.ClearSlot(L1_2, L3_2)
  Event.Delete(A0_2.eTimer)
end

Cleanup = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L3_2 = Object.GetVelocity
  L4_2 = A0_2.uVeh
  L3_2, L4_2, L5_2, L6_2 = L3_2(L4_2)
  L2_2 = Math.abs(L3_2, L4_2, L5_2, L6_2)
  L3_2 = A0_2.bStartBomb
  if not L3_2 then
    L3_2 = A0_2.kSpeedThreshold
    if L2_2 >= L3_2 then
      A0_2.bStartBomb = true
    end
    UpdateDisplay(L2_2, A0_2.fTimeLeft, A0_2)
    return
  end
  L3_2 = A0_2.kSpeedThreshold
  if L2_2 < L3_2 then
    L3_2 = A0_2.bMarked
    if L3_2 then
      A0_2.fTimeLeft = (A0_2.fMarkedTime - Sys.TimeStampGetElapsed(A0_2.uTimeStamp))
    else
      A0_2.bMarked = true
      Sys.TimeStampMark(A0_2.uTimeStamp)
      A0_2.fMarkedTime = A0_2.fTimeLeft
    end
  else
    A0_2.bMarked = nil
  end
  L3_2 = A0_2.fTimeLeft
  if L3_2 <= 0 then
    A0_2.fTimeLeft = 0
    KillVehicle(A0_2)
  end
  UpdateDisplay(L2_2, A0_2.fTimeLeft, A0_2)
end

Update = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  Sound.CueSound(A0_2.uVeh, "wpn_bomb_timer_01_finalstage")
  Event.Delete(A0_2.eTimer)
  L3_2 = {}
  L3_2[1] = 1
  L5_2 = {}
  L5_2[1] = A0_2
  Event.Create(Event.TimerRelative, L3_2, KillVehicle2, L5_2)
end

KillVehicle = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = A0_2.uVeh
  Sound.StopSound(L1_2, "wpn_bomb_timer_01_finalstage")
  MrxUtil.SpawnObject("fx_Explosion_HugeOil", L1_2)
  Object.Kill(L1_2)
  L4_2 = {}
  L4_2[1] = 2
  L6_2 = {}
  L6_2[1] = A0_2
  Event.Create(Event.TimerRelative, L4_2, Cleanup, L6_2)
end

KillVehicle2 = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L3_2 = "[green]"
  L4_2 = A2_2.bStartBomb
  if not L4_2 then
    L3_2 = "[white]"
  else
    L4_2 = A2_2.kSpeedThreshold
    if A0_2 < L4_2 then
      L3_2 = "[red]"
    else
      L4_2 = A2_2.kSpeedThreshold + 1.3888888
      if A0_2 < L4_2 then
        L3_2 = "[yellow]"
      end
    end
  end
  L4_2 = Math.floor(((100 * A1_2) / A2_2.kInitBombTime))
  L5_2 = "[white]"
  if not (L4_2 < 10) then
    L6_2 = A2_2.bMarked
    if not L6_2 then
      goto lbl_32
    end
  end
  L5_2 = "[red]"
  ::lbl_32::
  A0_2 = Math.floor((A0_2 * 3.6))
  L6_2 = Hud.ObjectiveTray
  L8_2 = {}
  L8_2.nSlot = 2
  L8_2.sText = (L3_2 .. A0_2 .. " Kph")
  L6_2.SetSlotToText(L6_2, L8_2)
  L6_2 = Hud.ObjectiveTray
  L8_2 = {}
  L8_2.nSlot = 3
  L8_2.sText = (L5_2 .. "[bar" .. L4_2 .. "]")
  L6_2.SetSlotToText(L6_2, L8_2)
end

UpdateDisplay = L0_1
