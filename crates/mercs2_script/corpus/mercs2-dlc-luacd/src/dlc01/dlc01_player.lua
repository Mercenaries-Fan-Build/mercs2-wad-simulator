local L0_1, L1_1, L2_1, L3_1, L4_1, L5_1, L6_1, L7_1, L8_1, L9_1, L10_1, L11_1, L12_1, L13_1, L14_1, L15_1, L16_1
import("MrxCoop", false)
import("MrxGui", false)
import("MrxGuiManager", false)
import("MrxGuiShellBootstrap", false)
import("MrxPlayState", false)
import("MrxPmc", false)
import("MrxUtil", false)
import("WifPmcInterior", false)
import("MrxMissionFlow", false)
import("MrxState", false)
import("MrxTransit", false)
import("MrxActionHijack", false)
import("MrxHqManager", false)
import("MrxVoSequence", false)
import("Hero", false)
import("MrxStatsManager", false)
import("Munitions", false)
import("MrxSound", false)
import("MrxGuiHudMessage", false)
L0_1 = {}
L1_1 = {}
L1_1.base = "mattias"
L2_1 = {}
L2_1[1] = "mattiasupgrade1"
L2_1[2] = "mattiasupgrade2"
L2_1[3] = "mattiasupgrade3"
L1_1.templates = L2_1
L2_1 = {}
L2_1[1] = "pmc_hum_mattias_v3"
L2_1[2] = "pmc_hum_mattias_v2"
L2_1[3] = "pmc_hum_mattias_v4"
L2_1[4] = "pmc_hum_mattias_chickensuit"
L2_1[5] = "pmc_hum_helipilot_unlockable"
L2_1[6] = "pmc_hum_proppilot_unlockable"
L2_1[7] = "pmc_hum_obama"
L2_1[8] = "pmc_hum_sarah"
L2_1[9] = "pmc_hum_mattias_v5"
L2_1[10] = "gr_hum_starter_1"
L2_1[11] = "pmc_hum_stealth"
L2_1[12] = "pmc_hum_hoang"
L2_1[13] = "gr_hum_boss_fake"
L2_1[14] = "oc_hum_pilot"
L1_1.models = L2_1
L2_1 = {}
L2_1.base = "chris"
L3_1 = {}
L3_1[1] = "chrisupgrade1"
L3_1[2] = "chrisupgrade2"
L3_1[3] = "chrisupgrade3"
L2_1.templates = L3_1
L3_1 = {}
L3_1[1] = "pmc_hum_chris_v2"
L3_1[2] = "pmc_hum_chris_v3"
L3_1[3] = "pmc_hum_chris_v4"
L3_1[4] = "pmc_hum_chris_chickensuit"
L3_1[5] = "pr_hum_boss"
L3_1[6] = "pmc_hum_blanco"
L3_1[7] = "pmc_hum_obama"
L3_1[8] = "pmc_hum_sarah"
L2_1.models = L3_1
L3_1 = {}
L3_1.base = "jen"
L4_1 = {}
L4_1[1] = "jenupgrade1"
L4_1[2] = "jenupgrade2"
L4_1[3] = "jenupgrade3"
L3_1.templates = L4_1
L4_1 = {}
L4_1[1] = "pmc_hum_jen_v3"
L4_1[2] = "pmc_hum_jen_v5"
L4_1[3] = "pmc_hum_jen_v2"
L4_1[4] = "pmc_hum_jen_v4"
L4_1[5] = "pmc_hum_jen_chickensuit"
L4_1[6] = "pmc_hum_fiona_unlockable"
L4_1[7] = "pmc_hum_mechanic"
L4_1[8] = "pmc_hum_obama"
L4_1[9] = "pmc_hum_sarah"
L4_1[10] = "pmc_hum_diablo"
L3_1.models = L4_1
L0_1[1] = L1_1
L0_1[2] = L2_1
L0_1[3] = L3_1
L1_1 = {}
L1_1[1] = "Mattias.Misc.Joiner02"
L1_1[2] = "Mattias.Misc.Joinee01"
L1_1[3] = "Mattias.Misc.Joinee02"
L1_1[4] = "Mattias.Misc.Joiner01"
L2_1 = {}
L2_1[1] = "Mattias.Misc.Joinee01"
L2_1[2] = "Mattias.Misc.Joinee02"
L2_1[3] = "Mattias.Misc.Joiner01"
L3_1 = {}
L3_1[1] = "Jen.Co_Op.Joiner01"
L3_1[2] = "Jen.Co_Op.Joiner02"
L4_1 = {}
L4_1[1] = "Jen.Co_Op.Joinee01"
L5_1 = {}
L5_1[1] = "Chris.Misc.Joiner01"
L5_1[2] = "Chris.Misc.Joiner02"
L6_1 = {}
L6_1[1] = "Chris.Misc.Joinee01"
L7_1 = {}
L7_1[1] = "Jen.Co_Op.Dropped01"
L7_1[2] = "Jen.Co_Op.Dropped02"
L8_1 = {}
L8_1[1] = "Mattias.Misc.Abandoned01"
L8_1[2] = "Mattias.Misc.Abandoned02"
L9_1 = {}
L9_1[1] = "Chris.Misc.Abandoned01"
L9_1[2] = "Chris.Misc.Abandoned02"

function L10_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2
  L0_2 = 0
  L1_2 = Player.GetMaximumPlayers() - 1
  L2_2 = 1
  for L3_2 = L0_2, L1_2, L2_2 do
    Player.CreatePlayer(L3_2)
  end
end

Init = L10_1

function L10_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2
  MrxGuiManager.DeleteAllGuis()
  L0_2 = 0
  L1_2 = Player.GetMaximumPlayers() - 1
  L2_2 = 1
  for L3_2 = L0_2, L1_2, L2_2 do
    Player.DestroyPlayer(L3_2)
  end
  Player.ClearPlayerDB()
end

Deinit = L10_1

function L10_1()
  local L0_2, L1_2
  Player.SetPlayerJoinedCallback(OnPlayerJoined)
  Player.SetPlayerLeftCallback(OnPlayerLeft)
end

Start = L10_1

function L10_1()
  local L0_2, L1_2
  Player.RemovePlayerJoinedCallback()
  Player.RemovePlayerLeftCallback()
end

Reset = L10_1

function L10_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2
  _fLocalPlayerJoinedFunc = A0_2
  _tLocalPlayerJoinedArgs = A1_2
  L2_2 = Player.GetLocalPlayer()
  if L2_2 then
    L2_2 = _fLocalPlayerJoinedFunc
    if L2_2 ~= nil then
      MrxUtil.CallWithOptionalArgs(_fLocalPlayerJoinedFunc, _tLocalPlayerJoinedArgs)
    end
  end
end

SetLocalPlayerJoinedCallback = L10_1

function L10_1(A0_2)
  local L1_2
  _tSpawnLocations = A0_2
end

SetSpawnLocations = L10_1

function L10_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L1_2 = type(A0_2)
  if L1_2 ~= "table" then
    return A0_2
  end
  L1_2 = A0_2.iIndex
  if not L1_2 then
    return
  end
  L2_2 = L0_1[L1_2]
  if not L2_2 then
    return
  end
  L3_2 = A0_2.iUpgrade
  L4_2 = L2_2.templates
  if L3_2 and L4_2 then
    L5_2 = L4_2[L3_2]
    if L5_2 then
      goto lbl_26
    end
  end
  L5_2 = L2_2.base
  ::lbl_26::
  L6_2 = A0_2.iCostume
  L7_2 = L2_2.models
  if L6_2 and L7_2 then
    L8_2 = L7_2[L6_2]
    if L8_2 then
      goto lbl_36
    end
  end
  L8_2 = nil
  ::lbl_36::
  L9_2 = L5_2
  L10_2 = L8_2
  return L9_2, L10_2
end

GetTemplateAndModelName = L10_1

function L10_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  L2_2 = MrxUtil.GetCharacterIdentity(A0_2)
  L3_2 = {}
  L3_2.mattias = 1
  L3_2.chris = 2
  L3_2.jennifer = 3
  L5_2 = L0_1[L3_2[L2_2]].models[A1_2]
  return L5_2
end

GetModelAtIndex = L10_1

function L10_1(A0_2, A1_2, A2_2, A3_2, A4_2)
  local L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2
  if A3_2 then
    A2_2.iCostume = Player.GetProfileCostume()
  end
  L5_2 = GetTemplateAndModelName
  L6_2 = A2_2
  L5_2, L6_2 = L5_2(L6_2)
  if not L5_2 then
    L5_2 = GetSelectedCharacter(A0_2)
  end
  L7_2 = Player.GetPlayerStart()
  L8_2 = _tSpawnLocations
  if L8_2 then
    L7_2 = _tSpawnLocations[(A0_2 + 1)]
  end
  L8_2 = CreatePlayerCharacter(A3_2, A0_2, L5_2, L7_2)
  if L6_2 then
    L11_2 = {}
    L11_2[1] = L8_2
    L11_2[2] = "awake"
    L13_2 = {}
    L13_2[1] = L8_2
    L13_2[2] = L6_2
    Event.Create(Event.ObjectHibernation, L11_2, Player.SetOutfit, L13_2)
    L9_2 = Net.IsClient()
    if L9_2 and not A3_2 then
      SetRemoteOutfit(A2_2.iCostume)
    end
    if L6_2 == "pmc_hum_sarah" then
      Human.SetChatterSet(L8_2, "Palin_Palin01")
    elseif L6_2 == "pmc_hum_obama" then
      Human.SetChatterSet(L8_2, "Obama_Obama01")
    end
  end
  if A3_2 then
    Player.BindToLocal(A0_2, A4_2)
  else
    Player.BindToRemote(A0_2)
  end
  if A3_2 then
    MrxGuiManager.CreateGui(Player.GetPlayer(A0_2))
  end
  Ai.AddSubject(L8_2)
  if A3_2 then
    L9_2 = _fLocalPlayerJoinedFunc
    if L9_2 ~= nil then
      MrxUtil.CallWithOptionalArgs(_fLocalPlayerJoinedFunc, _fLocalPlayerJoinedArgs)
    end
  end
  L9_2 = Player.GetPlayer(A0_2)
  L10_2 = MrxPlayState.GetCurrentMission()
  if L10_2 then
    L10_2.OnPlayerJoined(L10_2, A0_2, L9_2, L8_2)
  end
  L13_2 = {}
  L13_2[1] = L9_2
  L13_2[2] = L8_2
  Event.Post("mpPlayerJoin", L13_2)
  L11_2 = Net.IsClient()
  if L11_2 and not A3_2 then
    L13_2 = {}
    L13_2[1] = "WaitForStreaming"
    L13_2[2] = "exit"
    L15_2 = {}
    L15_2[1] = "MrxPlayer"
    L15_2[2] = NETEVENT_CLIENTDONESTREAMING
    Event.Create(Event.GameStateChange, L13_2, Net.SendCustomEvent, L15_2, true)
    L11_2 = Player.GetFuelCapacity()
    if 300 < L11_2 then
      L12_2 = Player.GetFuelCapacity
      L12_2, L13_2, L14_2, L15_2, L16_2, L17_2 = L12_2()
      MrxPmc.SetFuelCapacity(L12_2, L13_2, L14_2, L15_2, L16_2, L17_2)
    end
    L11_2 = Player.GetFuel()
    L12_2 = Player.GetFuelCapacity()
    if L11_2 > L12_2 then
      L12_2 = Player.GetFuelCapacity
      L12_2, L13_2, L14_2, L15_2, L16_2, L17_2 = L12_2()
      Player.SetFuel(L12_2, L13_2, L14_2, L15_2, L16_2, L17_2)
    end
  end
  L11_2 = Net.IsServer()
  if L11_2 and not A3_2 then
    Munitions.OnPlayerJoined()
  end
end

OnPlayerJoined = L10_1

function L10_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L0_2 = Net.IsClient()
  if not L0_2 then
    L0_2 = GetSelectedCharacter(0)
    L1_2 = GetSelectedCharacter(1)
    sClientVOString = nil
    sHostVOString = nil
    if L1_2 == "mattias" then
      L3_2 = table.getn
      L4_2 = L1_1
      L3_2, L4_2, L5_2, L6_2 = L3_2(L4_2)
      sClientVOString = L1_1[math.randi(L3_2, L4_2, L5_2, L6_2)]
    elseif L1_2 == "jen" then
      L2_2 = MrxMissionFlow._tActiveMissions.VzaCon001
      if L2_2 == nil then
        table.insert(L3_1, "Jen.Co_Op.Joinee02")
      end
      L3_2 = table.getn
      L4_2 = L3_1
      L3_2, L4_2, L5_2, L6_2 = L3_2(L4_2)
      sClientVOString = L3_1[math.randi(L3_2, L4_2, L5_2, L6_2)]
    elseif L1_2 == "chris" then
      L2_2 = MrxMissionFlow._tActiveMissions.VzaCon001
      if L2_2 == nil then
        table.insert(L5_1, "Chris.Misc.Joinee02")
      end
      L3_2 = table.getn
      L4_2 = L5_1
      L3_2, L4_2, L5_2, L6_2 = L3_2(L4_2)
      sClientVOString = L5_1[math.randi(L3_2, L4_2, L5_2, L6_2)]
    end
    if L0_2 == "mattias" then
      L3_2 = table.getn
      L4_2 = L2_1
      L3_2, L4_2, L5_2, L6_2 = L3_2(L4_2)
      sHostVOString = L2_1[math.randi(L3_2, L4_2, L5_2, L6_2)]
    elseif L0_2 == "jen" then
      L2_2 = MrxMissionFlow._tActiveMissions.VzaCon001
      if L2_2 == nil then
        table.insert(L4_1, "Jen.Co_Op.Joinee02")
      end
      L3_2 = table.getn
      L4_2 = L4_1
      L3_2, L4_2, L5_2, L6_2 = L3_2(L4_2)
      sHostVOString = L4_1[math.randi(L3_2, L4_2, L5_2, L6_2)]
    elseif L0_2 == "chris" then
      L2_2 = MrxMissionFlow._tActiveMissions.VzaCon001
      if L2_2 == nil then
        table.insert(L6_1, "Chris.Misc.Joinee02")
      end
      L3_2 = table.getn
      L4_2 = L6_1
      L3_2, L4_2, L5_2, L6_2 = L3_2(L4_2)
      sHostVOString = L6_1[math.randi(L3_2, L4_2, L5_2, L6_2)]
    end
    L2_2 = sClientVOString
    if L2_2 ~= nil then
      L2_2 = sHostVOString
      if L2_2 ~= nil then
        goto lbl_139
      end
    end
    do return end
    ::lbl_139::
    L3_2 = {}
    L4_2 = {}
    L4_2[1] = sClientVOString
    L5_2 = {}
    L5_2[1] = sHostVOString
    L3_2[1] = L4_2
    L3_2[2] = L5_2
    MrxVoSequence.Start(L3_2)
  end
end

PlayJoinVO = L10_1

function L10_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2
  L2_2 = table.getn
  L3_2 = A0_2
  L2_2, L3_2, L4_2, L5_2 = L2_2(L3_2)
  sVOString = A0_2[math.randi(L2_2, L3_2, L4_2, L5_2)]
  L3_2 = {}
  L4_2 = {}
  L4_2[1] = sVOString
  L3_2[1] = L4_2
  MrxVoSequence.Start(L3_2)
end

PlayRandomLeftVO = L10_1

function L10_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L3_2 = Player.GetLocalPlayerId()
  if 0 <= L3_2 then
    L3_2 = MrxSound.ExitingGame()
    if not L3_2 then
      L4_2 = Player.GetLocalPlayerId
      L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2 = L4_2()
      sTemplateName = GetSelectedCharacter(L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2)
      L3_2 = sTemplateName
      if L3_2 == "mattias" then
        PlayRandomLeftVO(L8_1)
      else
        L3_2 = sTemplateName
        if L3_2 == "jen" then
          PlayRandomLeftVO(L7_1)
        else
          L3_2 = sTemplateName
          if L3_2 == "chris" then
            PlayRandomLeftVO(L9_1)
          end
        end
      end
    end
  end
  L3_2 = Player.GetPlayer(A0_2)
  L4_2 = Player.GetCharacter(L3_2)
  L7_2 = {}
  L7_2[1] = L3_2
  L7_2[2] = L4_2
  Event.Post("mpPlayerLeft", L7_2)
  L5_2 = MrxPlayState.GetCurrentMission()
  if L5_2 then
    L5_2.OnPlayerLeft(L5_2, A0_2, L3_2, L4_2)
  end
  Vehicle.CancelHijack(L4_2)
  MrxGuiManager.DeleteGui(L3_2)
  Ai.RemoveSubject(L4_2)
  Player.Unbind(A0_2)
  DestroyPlayerCharacter(A0_2)
  Event.Delete(_evReviveNag)
  Event.Delete(_evCancelTimer)
  L6_2 = AreAnyHeroesAlive()
  if not L6_2 then
    L7_2 = MrxSound.ExitingGame()
    if not L7_2 then
      if L5_2 then
        L5_2.Cancel(L5_2)
      elseif A0_2 == 1 then
        PlayerDied(A0_2, L4_2)
      end
    end
  end
end

OnPlayerLeft = L10_1

function L10_1(A0_2, A1_2, A2_2, A3_2, A4_2)
  local L5_2, L6_2, L7_2, L8_2, L9_2
  Event.Delete(_evCancelTimer)
  Event.Delete(_evReviveNag)
  Pg.RemoveContextAction(A1_2)
  L5_2 = Player.IsRemote(A0_2)
  if L5_2 then
    Net.SendEvent_RevivePlayer(A0_2)
  elseif A1_2 then
    L5_2 = Object.IsAlive(A1_2)
    if not L5_2 then
      Object.Revive(A1_2, 0.5)
      if A2_2 and A3_2 and A4_2 then
        Object.DisablePhysics(A1_2)
        Object.SetPosition(A1_2, A2_2, A3_2, A4_2)
        L5_2 = Event.Create
        L6_2 = Event.TimerRelative
        L7_2 = {}
        L7_2[1] = 0.5
        
        function L8_2()
          local L0_3, L1_3
          Object.EnablePhysics(A1_2)
        end
        
        L5_2(L6_2, L7_2, L8_2)
      end
    end
  end
end

RequestPlayerRevive = L10_1

function L10_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L2_2 = MrxPlayState.GetCurrentMission()
  L3_2 = Net.IsClient()
  if L3_2 then
    L3_2 = Player.IsBoundaryDeath(A1_2)
    if L3_2 then
      Net.SendCustomEvent("MrxPlayer", NETEVENT_CLIENT_OUT_BOUNDARY_DEATH, {})
    end
  end
  Event.Delete(_evCancelTimer)
  Event.Delete(_evReviveNag)
  L3_2 = AreAnyHeroesAlive()
  if L3_2 then
    L3_2 = Object.IsAlive(A1_2)
    if not L3_2 then
      L3_2 = Net.IsClient()
      if not L3_2 then
        L3_2 = Player.IsBoundaryDeath(A1_2)
        if L3_2 then
          if L2_2 then
            L2_2.Cancel(L2_2)
          else
            MoveToSickbay()
          end
        else
          Pg.AddContextAction(A1_2, "[ContextAction.Revive]", 2, 0, 0, 0, 0)
          L5_2 = {}
          L5_2[1] = Player.GetAnyCharacter()
          L5_2[2] = A1_2
          L7_2 = {}
          L7_2[1] = A0_2
          L7_2[2] = A1_2
          Event.Create(Event.ContextAction, L5_2, RequestPlayerRevive, L7_2)
          VO.Cue(0, "Fiona.Misc.Revive01")
          if L2_2 then
            L5_2 = {}
            L5_2[1] = 30
            L7_2 = {}
            L7_2[1] = 0
            L7_2[2] = "Fiona.Misc.Revive02"
            _evReviveNag = Event.Create(Event.TimerRelative, L5_2, VO.Cue, L7_2)
          else
            L5_2 = {}
            L5_2[1] = 30
            L7_2 = {}
            L7_2[1] = 0
            L7_2[2] = "Fiona.Misc.Revive01"
            _evReviveNag = Event.Create(Event.TimerRelative, L5_2, VO.Cue, L7_2)
          end
          L3_2 = Event.Create
          L4_2 = Event.TimerRelative
          L5_2 = {}
          L5_2[1] = 60
          
          function L6_2()
            local L0_3, L1_3
            L0_3 = L2_2
            if L0_3 then
              L0_3 = L2_2
              L0_3.Cancel(L0_3)
            else
              MoveToSickbay()
            end
          end
          
          _evCancelTimer = L3_2(L4_2, L5_2, L6_2)
        end
      end
    end
  else
    L3_2 = Net.IsClient()
    if L3_2 then
      return
    end
    if L2_2 then
      L2_2.Cancel(L2_2)
    else
      L5_2 = {}
      L5_2[1] = 2
      L7_2 = {}
      L7_2[1] = 1
      Event.Create(Event.TimerRelative, L5_2, _DialogBoxDismissed, L7_2)
    end
    L3_2 = Player.GetLocalPlayer()
    if L3_2 then
      L4_2 = Player.GetLocalPlayer
      L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2 = L4_2()
      Player.ClearGPS(L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2)
    end
  end
  MrxStatsManager.IncreaseDeathCounter()
end

PlayerDied = L10_1

function L10_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L0_2 = AreAnyHeroesAlive()
  if not L0_2 then
    L1_2 = Player.GetPrimaryPlayer()
    L2_2 = "[Fanfare.Common.You_Have_Died]"
    L3_2 = {}
    L4_2 = "[Fanfare.Common.Death_Continue]"
    L5_2 = "[Fanfare.Common.Quit]"
    L3_2[1] = L4_2
    L3_2[2] = L5_2
    MrxGui.DisplayDialogBox(L1_2, L2_2, L3_2, 1, _DialogBoxDismissed, nil, nil, nil, nil, nil, nil, 2)
  end
end

_DisplayOnDeathDialogBox = L10_1

function L10_1(A0_2)
  local L1_2, L2_2
  if A0_2 == 1 then
    L1_2 = string.lower(Sys.GetLevelName())
    if L1_2 == "vz" then
      MoveToSickbay()
    else
    end
  end
end

_DialogBoxDismissed = L10_1

function L10_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L0_2 = Player.GetAllPlayers()
  L1_2 = ipairs
  L2_2 = L0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L7_2 = Object.IsAlive(Player.GetCharacter(L5_2))
    if L7_2 then
      L7_2 = Player.InCinematicMode(L5_2)
      if not L7_2 then
        goto lbl_26
      end
    end
    L7_2 = false
    do return L7_2 end
    ::lbl_26::
  end
  L1_2 = MrxTransit.IsInTransit()
  if L1_2 then
    L1_2 = false
    return L1_2
  end
  L1_2 = MrxActionHijack.IsInHijack()
  if L1_2 then
    L1_2 = false
    return L1_2
  end
  L1_2 = WifPmcInterior.IsUnlocked()
  if L1_2 then
    L1_2 = WifPmcInterior.IsInside()
    if not L1_2 then
      goto lbl_54
    end
  end
  L1_2 = false
  do return L1_2 end
  ::lbl_54::
  L1_2 = MrxHqManager.IsInside()
  if L1_2 then
    L1_2 = false
    return L1_2
  end
  L1_2 = MrxState.IsLocked()
  if L1_2 then
    L1_2 = false
    return L1_2
  end
  L1_2 = Net.IsClient()
  if L1_2 then
    L1_2 = false
    return L1_2
  end
  L1_2 = true
  return L1_2
end

CanMedEvac = L10_1

function L10_1()
  local L0_2, L1_2, L2_2, L3_2
  L0_2 = MrxPlayState.GetCurrentMission()
  if L0_2 then
    L0_2.SetCancelByMedEvac(L0_2, true)
    L0_2.Cancel(L0_2)
    MrxVoSequence.Stop()
    VO.CancelAll()
  else
    MoveToSickbay()
  end
  L2_2 = Player.GetLocalPlayer
  L2_2, L3_2 = L2_2()
  Player.ClearGPS(L2_2, L3_2)
  MrxStatsManager.IncreaseMedevacCounter()
end

MedEvac = L10_1

function L10_1()
  local L0_2, L1_2, L2_2
  L0_2 = WifPmcInterior.IsInside()
  if L0_2 then
    L0_2 = false
    return L0_2
  end
  MrxVoSequence.Stop()
  VO.CancelAll()
  MrxState.Enter(MrxState.STATE_WAITFORGAME, _MoveToSickbayBegin)
  L0_2 = true
  return L0_2
end

MoveToSickbay = L10_1

function L10_1()
  local L0_2, L1_2, L2_2, L3_2
  L0_2 = Player.GetLocalCharacter()
  if L0_2 then
    L1_2 = Human.IsCarrying(L0_2)
    if L1_2 then
      Human.Drop(L0_2, true)
    end
    L1_2 = Object.IsAlive(L0_2)
    if not L1_2 then
      ResetWeapons(L0_2)
    end
  end
end

ResetLocalCharacter = L10_1

function L10_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L0_2 = Player.GetAllPlayers()
  L1_2 = ipairs
  L2_2 = L0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L7_2 = Human.IsGrappling(Player.GetCharacter(L5_2))
    if L7_2 then
      Human.StopGrappling(L6_2)
    end
    L7_2 = Vehicle.GetFromRider(L6_2)
    if L7_2 then
      Vehicle.Exit(L7_2, L6_2, true)
    end
  end
  ResetLocalCharacter()
  L1_2 = Net.IsClient()
  if L1_2 then
    Net.SendCustomEvent("MrxPlayer", NETEVENT_CLIENTSELECTMEDEVAC, {})
    return
  end
  L1_2 = Net.IsServer()
  if L1_2 then
    Net.SendCustomEvent("MrxPlayer", NETEVENT_HOST_SELECT_MEDEVAC, {})
  end
  WifPmcInterior.Enter(true, 2)
  L3_2 = {}
  L3_2[1] = "WaitForStreaming"
  L3_2[2] = "exit"
  Event.Create(Event.GameStateChange, L3_2, _CompleteMove)
  MrxState.Exit(MrxState.STATE_WAITFORGAME, _MoveToSickbayEnd)
end

_MoveToSickbayBegin = L10_1

function L10_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L0_2 = Player.GetAllPlayers()
  L1_2 = ipairs
  L2_2 = L0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = Player.GetCharacter(L5_2)
    Object.SetTransformToObject(L6_2, ("Sickbay Hero Respawn " .. L4_2))
    Camera.SetYaw(Player.GetCamera(L5_2), 80)
    Hero.EndSurvivalMode(L5_2, L6_2)
  end
  RiseFromYourGrave()
  Event.Post("MedevacComplete", L0_2)
end

_CompleteMove = L10_1

function L10_1()
  local L0_2, L1_2, L2_2, L3_2
  MrxPmc.AddCashQty(-GetMedEvacCost(), true, "[Generic.Medevacs]")
end

_MoveToSickbayEnd = L10_1

function L10_1()
  local L0_2, L1_2
  L0_2 = 10000
  return L0_2
end

GetMedEvacCost = L10_1

function L10_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  L2_2 = A1_2 or nil
  if not A1_2 then
    L2_2 = "Pistol"
  end
  L5_2 = Pg.GetGuidByName(L2_2)
  L6_2 = Pg.GetGuidByName("Grenade")
  L7_2 = Pg.GetGuidByName("C4")
  L10_2 = {}
  L10_2[1] = L5_2
  L10_2[2] = L6_2
  L10_2[3] = L7_2
  Human.Inventory.SetAllWeapons(A0_2, L10_2)
end

ResetWeapons = L10_1

function L10_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L0_2 = Player.GetAllPlayers()
  L1_2 = ipairs
  L2_2 = L0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = Player.GetCharacter(L5_2)
    L7_2 = Player.GetPrimaryCharacter()
    if L6_2 == L7_2 then
      L7_2 = Object.IsAlive(L6_2)
      if not L7_2 then
        Object.Revive(L6_2)
    end
    else
      L7_2 = Player.GetSecondaryCharacter()
      if L6_2 == L7_2 then
        L7_2 = Object.IsAlive(L6_2)
        if not L7_2 then
          Net.SendEvent_RevivePlayer(1)
        end
      end
    end
  end
end

RiseFromYourGrave = L10_1

function L10_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L2_2 = Object.IsAlive(A1_2)
  if L2_2 then
    L4_2 = {}
    L4_2[1] = A1_2
    L6_2 = {}
    L6_2[1] = A0_2
    L6_2[2] = A1_2
    Event.CreatePersistent(Event.ObjectDeath, L4_2, PlayerDied, L6_2)
  else
    L4_2 = {}
    L4_2[1] = 2
    L6_2 = {}
    L6_2[1] = A0_2
    L6_2[2] = A1_2
    Event.Create(Event.TimerRelative, L4_2, OnPlayerInit, L6_2)
  end
end

OnPlayerInit = L10_1

function L10_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2
  L4_2 = 0
  L5_2 = 0
  L6_2 = 0
  L7_2 = 0
  if not A0_2 then
    L8_2 = Player.GetLocalCharacter()
    if L8_2 then
      L9_2 = Object.GetPosition
      L10_2 = L8_2
      L9_2, L10_2, L11_2 = L9_2(L10_2)
      L6_2 = L11_2
      L5_2 = L10_2
      L4_2 = L9_2
      L7_2 = Object.GetYaw(L8_2)
    end
  elseif A3_2 then
    L8_2 = type(A3_2)
    if L8_2 == "string" then
      L8_2 = Pg.GetGuidByName(A3_2)
      if L8_2 then
        L9_2 = Object.GetPosition
        L10_2 = L8_2
        L9_2, L10_2, L11_2 = L9_2(L10_2)
        L6_2 = L11_2
        L5_2 = L10_2
        L4_2 = L9_2
        L7_2 = Object.GetYaw(L8_2)
      end
    else
      L8_2 = type(A3_2)
      if L8_2 == "table" then
        L4_2 = A3_2[1]
        L5_2 = A3_2[2]
        L6_2 = A3_2[3]
        L7_2 = A3_2[4]
      end
    end
  end
  L8_2 = Pg.Spawn(A2_2, L4_2, L5_2, L6_2, L7_2, false, false, false)
  Player.AttachToCharacter(A1_2, L8_2)
  OnPlayerInit(A1_2, L8_2)
  return L8_2
end

CreatePlayerCharacter = L10_1

function L10_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  L1_2 = Player.GetPlayer(A0_2)
  if L1_2 == nil then
    return
  end
  L2_2 = Player.GetCharacter(L1_2)
  if L2_2 then
    Player.DetachFromCharacter(A0_2)
    Object.Remove(L2_2)
  end
end

DestroyPlayerCharacter = L10_1

function L10_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2
  L3_2 = Player.GetPlayer(A0_2)
  if L3_2 == nil then
    return
  end
  L4_2 = nil
  L5_2 = nil
  L6_2 = nil
  L7_2 = nil
  L8_2 = Player.GetCharacter(L3_2)
  if L8_2 then
    L9_2 = Object.GetPosition
    L10_2 = L8_2
    L9_2, L10_2, L11_2 = L9_2(L10_2)
    L6_2 = L11_2
    L5_2 = L10_2
    L4_2 = L9_2
    L7_2 = Object.GetYaw(L8_2)
    Player.DetachFromCharacter(A0_2)
    Object.Remove(L8_2)
  else
    L7_2 = 0
    L6_2 = 0
    L5_2 = 0
    L4_2 = 0
  end
  L8_2 = Pg.Spawn(A1_2, L4_2, L5_2, L6_2, L7_2, false, false, false)
  L9_2 = sModelName
  if L9_2 then
    Object.SetModelName(L8_2, sModelName)
  end
  Player.AttachToCharacter(A0_2, L8_2)
  OnPlayerInit(A0_2, L8_2)
  return L8_2
end

ChangePlayerCharacter = L10_1

function L10_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L2_2 = Player.GetCharacter(Player.GetPlayer(A0_2))
  if L2_2 == nil then
    L3_2 = MrxGuiShellBootstrap.GetSelectedCharacter()
    if not L3_2 then
      L3_2 = Sys.GetCharacterTemplate()
      if not L3_2 then
        L3_2 = "mattias"
      end
    end
    return L3_2
  end
  L4_2 = Object.HasLabel(Object.GetParent(L2_2), "Mattias")
  if L4_2 then
    L4_2 = "mattias"
    return L4_2
  else
    L4_2 = Object.HasLabel(L3_2, "Jennifer")
    if L4_2 then
      L4_2 = "jen"
      return L4_2
    else
      L4_2 = Object.HasLabel(L3_2, "Chris")
      if L4_2 then
        L4_2 = "chris"
        return L4_2
      end
    end
  end
end

GetSelectedCharacter = L10_1

function L10_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L0_2 = Player.GetAllPlayers()
  L1_2 = ipairs
  L2_2 = L0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = Player.GetCharacter(L5_2)
    if L6_2 then
      L7_2 = Object.IsAlive(L6_2)
      if L7_2 then
        L7_2 = true
        return L7_2
      end
    end
  end
  L1_2 = false
  return L1_2
end

AreAnyHeroesAlive = L10_1

function L10_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2
  L0_2 = {}
  L1_2 = Player.GetAllPlayers()
  L2_2 = ipairs
  L3_2 = L1_2
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    L8_2 = Human.Inventory.GetAllWeapons(Player.GetCharacter(L6_2), true)
    L9_2 = {}
    L10_2 = pairs
    L11_2 = L8_2
    L10_2, L11_2, L12_2 = L10_2(L11_2)
    for L13_2, L14_2 in L10_2, L11_2, L12_2 do
      L15_2 = {}
      L16_2 = Object.GetParent(L14_2)
      L17_2 = Weapon.GetReserveAmmo
      L18_2 = L14_2
      L17_2, L18_2 = L17_2(L18_2)
      L15_2[1] = L16_2
      L15_2[2] = L17_2
      L15_2[3] = L18_2
      L9_2[L13_2] = L15_2
    end
    L10_2 = {}
    L10_2.nHealth = Object.GetMaxHealth(L7_2)
    L10_2.tEquipment = L9_2
    L0_2[L5_2] = L10_2
  end
  return L0_2
end

SaveSingleton = L10_1

function L10_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2
  if not A0_2 then
    return
  end
  RiseFromYourGrave()
  L1_2 = Player.GetAllPlayers()
  L2_2 = ipairs
  L3_2 = L1_2
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  for L5_2, L6_2 in L2_2, L3_2, L4_2 do
    L7_2 = Player.GetCharacter(L6_2)
    L8_2 = A0_2[L5_2]
    if L8_2 then
      L9_2 = L8_2.tEquipment
      if L9_2 then
        function L9_2(A0_3, A1_3)
          local L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3, L9_3, L10_3, L11_3, L12_3, L13_3, L14_3, L15_3
          
          L2_3 = {}
          L3_3 = pairs
          L4_3 = A1_3
          L3_3, L4_3, L5_3 = L3_3(L4_3)
          for L6_3, L7_3 in L3_3, L4_3, L5_3 do
            L2_3[L6_3] = L7_3[1]
          end
          Human.Inventory.SetAllWeapons(A0_3, L2_3)
          L3_3 = Human.Inventory.GetAllWeapons(A0_3, true)
          L4_3 = pairs
          L5_3 = L3_3
          L4_3, L5_3, L6_3 = L4_3(L5_3)
          for L7_3, L8_3 in L4_3, L5_3, L6_3 do
            L9_3 = A1_3[L7_3]
            if L9_3 then
              L11_3 = {}
              L11_3[1] = L8_3
              L11_3[2] = "a"
              L13_3 = {}
              L15_3 = A1_3[L7_3][2]
              L13_3[1] = L8_3
              L13_3[2] = L15_3
              Event.Create(Event.ObjectHibernation, L11_3, Weapon.SetReserveAmmo, L13_3)
            else
            end
          end
        end
        
        _RestoreEquipment = L9_2
        L11_2 = {}
        L11_2[1] = L7_2
        L11_2[2] = "a"
        L13_2 = {}
        L13_2[1] = L7_2
        L13_2[2] = L8_2.tEquipment
        Event.Create(Event.ObjectHibernation, L11_2, _RestoreEquipment, L13_2)
      end
      Object.SetHealth(L7_2, L8_2.nHealth)
    end
  end
end

LoadSingleton = L10_1

function L10_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  L1_2 = Player.GetAllPlayers()
  L2_2 = ObjectFilter.Create()
  L3_2 = 0
  if not A0_2 then
    A0_2 = "Vehicle"
  end
  L6_2 = A0_2
  L2_2.SetFilter(L2_2, L6_2)
  L4_2 = pairs
  L5_2 = L1_2
  L4_2, L5_2, L6_2 = L4_2(L5_2)
  for L7_2, L8_2 in L4_2, L5_2, L6_2 do
    L10_2 = Vehicle.GetFromRider(Player.GetCharacter(L8_2))
    if L10_2 then
      L11_2 = L2_2.Eval(L2_2, L10_2)
      if L11_2 then
        L3_2 = L3_2 + 1
      end
    end
  end
  if 0 < L3_2 then
    return L3_2
  else
    L4_2 = false
    return L4_2
  end
end

IsInVehicle = L10_1

function L10_1(A0_2)
  local L1_2
  if A0_2 == nil then
    L1_2 = false
    return L1_2
  end
  L0_1 = nil
  L0_1 = A0_2
  L1_2 = true
  return L1_2
end

SetCharacterMap = L10_1

function L10_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2
  L4_2 = {}
  L4_2[1] = A0_2
  Net.SendCustomEvent("MrxPlayer", NETEVENT_CLIENTKILL, L4_2)
end

KilledByPlayer = L10_1

function L10_1(A0_2)
  local L1_2
  nRemoteOutfit = A0_2
end

SetRemoteOutfit = L10_1

function L10_1()
  local L0_2, L1_2
  L0_2 = nRemoteOutfit
  return L0_2
end

GetRemoteOutfit = L10_1
NETEVENT_CLIENTSELECTMEDEVAC = 0
NETEVENT_CLIENTDONESTREAMING = 1
NETEVENT_HOST_SELECT_MEDEVAC = 2
NETEVENT_CLIENTKILL = 3
NETEVENT_CLIENT_OUT_BOUNDARY_DEATH = 4

function L10_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2
  L2_2 = NETEVENT_CLIENTSELECTMEDEVAC
  if A0_2 == L2_2 then
    ResetLocalCharacter()
    WifPmcInterior.Enter(true, 2)
    L4_2 = {}
    L4_2[1] = "WaitForStreaming"
    L4_2[2] = "exit"
    Event.Create(Event.GameStateChange, L4_2, _CompleteMove)
  else
    L2_2 = NETEVENT_HOST_SELECT_MEDEVAC
    if A0_2 == L2_2 then
      ResetLocalCharacter()
    else
      L2_2 = NETEVENT_CLIENTDONESTREAMING
      if A0_2 == L2_2 then
        L2_2 = Event.Create
        L3_2 = Event.TimerRelative
        L4_2 = {}
        L4_2[1] = 4
        
        function L5_2()
          local L0_3, L1_3, L2_3
          L0_3 = Player.GetLocalCharacter()
          if L0_3 then
            L1_3 = Object.IsAlive(L0_3)
            if not L1_3 then
              Object.Revive(L0_3)
            end
          end
          PlayJoinVO()
          MrxGuiHudMessage.OnPlayerJoined()
        end
        
        L2_2(L3_2, L4_2, L5_2)
      else
        L2_2 = NETEVENT_CLIENTKILL
        if A0_2 == L2_2 then
          L2_2 = A1_2[1]
          if L2_2 then
            L4_2 = {}
            L4_2[1] = A1_2[1]
            Event.Post("ClientKill", L4_2)
          end
        else
          L2_2 = NETEVENT_CLIENT_OUT_BOUNDARY_DEATH
          if A0_2 == L2_2 then
            L2_2 = Net.IsClient()
            if not L2_2 then
              L2_2 = MrxPlayState.GetCurrentMission()
              Event.Delete(_evCancelTimer)
              Event.Delete(_evReviveNag)
              if L2_2 then
                L2_2.Cancel(L2_2)
              else
                MoveToSickbay()
              end
            end
          end
        end
      end
    end
  end
end

NetEventCallback = L10_1
