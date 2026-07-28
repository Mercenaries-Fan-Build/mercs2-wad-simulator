inherit("MrxTaskContract")
import("MrxTaskRace")
import("MrxTaskObjectiveDeliver")
import("MrxLayerManager")
import("MrxAchievements")
import("MrxAchievements")

function Activated(self)
  MrxTaskContract.Activated(self)
  Debug.Printf("********************* ALL CON 008: Mission Started!!!")
  uPlayer = Player.GetAnyCharacter()
  tPlayers = Player.GetAllPlayers()
  nPlayers = table.getn(tPlayers)
  nComp = self:GetNumCompletions() or 0
  MrxLayerManager.Add(vLayersStage)
  if nComp > 1 then
    nComp = 2
  end
  ChopperRace(self)
end

function ChopperRace(self)
  local sMissionName = "AllCon008"
  local vLayers = "VZ_state_AllCon008"
  local vLayersStage = "VZ_state_AllCon008_staging"
  self.oRaceObjective = self:CreateChild({
    sRaceMission = "AllCon008",
    sName = "ChopperRace",
    sModuleName = "MrxTaskRace",
    vTgtInclude = tCopters,
    bUseTripWires = true,
    sGateType = "ring",
    tTimerParams = {
      nStartTime = 20 - nComp * 4,
      nStep = 1
    },
    nAddTime = 9 - nComp,
    tCourseLocs = {
      sMissionName .. "_checkpoint000",
      sMissionName .. "_checkpoint010",
      sMissionName .. "_checkpoint020",
      sMissionName .. "_checkpoint025",
      sMissionName .. "_checkpoint030",
      sMissionName .. "_checkpoint040",
      sMissionName .. "_checkpoint045",
      sMissionName .. "_checkpoint050",
      sMissionName .. "_checkpoint070",
      sMissionName .. "_checkpoint080",
      sMissionName .. "_checkpoint090",
      sMissionName .. "_checkpoint100",
      sMissionName .. "_checkpoint110",
      sMissionName .. "_checkpoint115",
      sMissionName .. "_checkpoint120",
      sMissionName .. "_checkpoint200",
      sMissionName .. "_checkpoint202",
      sMissionName .. "_checkpoint205",
      sMissionName .. "_checkpoint210",
      sMissionName .. "_checkpoint220",
      sMissionName .. "_checkpoint270",
      sMissionName .. "_checkpoint280",
      sMissionName .. "_checkpoint290",
      sMissionName .. "_checkpoint300",
      sMissionName .. "_checkpoint310",
      sMissionName .. "_checkpoint320",
      sMissionName .. "_checkpoint330",
      sMissionName .. "_checkpoint340",
      sMissionName .. "_checkpoint350",
      sMissionName .. "_checkpoint370",
      sMissionName .. "_checkpoint380",
      sMissionName .. "_checkpoint390",
      sMissionName .. "_checkpoint400",
      sMissionName .. "_checkpoint410",
      sMissionName .. "_checkpoint440",
      sMissionName .. "_checkpoint500"
    },
    vVoSeqOnAdd = {
      "Fiona-In-Mission-MinorContract-All08-01"
    },
    fOnComplete = function()
      local uWinner = self.oRaceObjective._uWinner
      if uWinner then
        MrxAchievements.AchievementAddCount("ACHIEVEMENT_HIGHWAY_TO_HELL", 1, uWinner, true)
      end
      local isMultiplayer = Player.GetCurrentPlayers()
      if isMultiplayer == 2 then
        local uWinner = self.oRaceObjective._uWinner
        MrxAchievements.NetGrantAchievement("ACHIEVEMENT_WHEELS_OF_STEEL", uWinner)
      end
      MrxLayerManager.Remove(vLayersStage)
      self:Complete()
    end,
    fOnCancel = function()
      self:Cancel()
    end
  })
  eCueMusic = self:_CreateEvent(Event.ObjectInSeat, {
    uPlayer,
    oCopter01,
    "d",
    "e"
  }, MrxMusic.PlaySpecialMusic, {
    "mu_mission_allcon008_01"
  })
end

function VehicleCheck(self)
  nCopters = nCopters - 1
  Debug.Printf("*###*###*###*###*###* There is now only " .. nCopters .. " helicopter(s) available for " .. nPlayers .. " players!!! *###*###*###*###*###*")
  if nCopters < nPlayers then
    self.Cancel(self)
  end
end

function LoadAssets(self)
  vLayersMain = "VZ_state_AllCon008"
  vLayersStage = "VZ_state_AllCon008_staging"
  local tLayersToAdd = {
    "VZ_state_AllCon008",
    "VZ_state_AllCon008_staging"
  }
  MrxLayerManager.Add(tLayersToAdd, GetCopters, {self})
end

function GetCopters(self)
  if Net.DoneReloadingLayers then
    Net.DoneReloadingLayers()
  end
  local uSpawn01 = Pg.GetGuidByName("AllCon008_SpawnCopter01")
  local uSpawn02 = Pg.GetGuidByName("AllCon008_SpawnCopter02")
  local x1, y1, z1 = Object.GetPosition(uSpawn01)
  local x2, y2, z2 = Object.GetPosition(uSpawn02)
  local nYaw1 = Object.GetYaw(uSpawn01)
  local nYaw21 = Object.GetYaw(uSpawn02)
  local nPlayers = Player.GetCurrentPlayers()
  if nPlayers == 2 then
    oCopter01 = Pg.Spawn("Coanda Transport", x1, y1, z1, nYaw1, true, true)
    oCopter02 = Pg.Spawn("Coanda Transport", x2, y2, z2, nYaw2, true, true)
    tCopters = {oCopter01, oCopter02}
  else
    oCopter01 = Pg.Spawn("Coanda Transport", x1, y1, z1, nYaw1, true, true)
    tCopters = {oCopter01}
  end
  nCopters = table.getn(tCopters)
  eCopterReady = Event.Create(Event.ObjectHibernation, {oCopter01, "awake"}, self.AssetsLoaded, {self})
end

function Cleanup(self)
  MrxMusic.StopSpecialMusic()
  MrxLayerManager.MarkForRemoval(vLayersStage)
  for i, uVeh in ipairs(tCopters) do
    Debug.Printf("\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183 Setting up removal events for " .. tostring(uVeh))
    eRemoveCopter = Event.Create(Event.ObjectHibernation, {uVeh, "hibernated"}, Object.Remove, {uVeh})
  end
  MrxTaskContract.Cleanup(self)
end
