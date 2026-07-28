inherit("MrxTaskContract")
import("MrxTaskRace")
import("MrxTaskObjectiveDeliver")
import("MrxLayerManager")
import("MrxAchievements")

function LoadAssets(self)
  vLayersMain = "VZ_state_OilCon005"
  vLayersStage = "VZ_state_OilCon005_staging"
  vLayersBonus = "VZ_state_OilCon005_Bonus"
  nComp = self:GetNumCompletions() or 0
  if nComp > 1 then
    nComp = 2
  end
  local tLayersToAdd = {vLayersMain, vLayersStage}
  MrxLayerManager.Add(tLayersToAdd, GetSportsCars, {self})
end

function Activated(self)
  MrxTaskContract.Activated(self)
  self:CarRace(tCars)
end

function CarRace(self, tVeh)
  local sMissionName = "OilCon005"
  self.oRaceObjective = self:CreateChild({
    sRaceMission = "OilCon005",
    sName = "CarRace",
    sModuleName = "MrxTaskRace",
    tTimerParams = {
      nStartTime = 8 - nComp * 2
    },
    vTgtInclude = tVeh,
    nAddTime = 7 - nComp,
    tCourseLocs = {
      sMissionName .. "_checkpoint010",
      sMissionName .. "_checkpoint020",
      sMissionName .. "_checkpoint030",
      sMissionName .. "_checkpoint040",
      sMissionName .. "_checkpoint050",
      sMissionName .. "_checkpoint060",
      sMissionName .. "_checkpoint070",
      sMissionName .. "_checkpoint080",
      sMissionName .. "_checkpoint090",
      sMissionName .. "_checkpoint100",
      sMissionName .. "_checkpoint110",
      sMissionName .. "_checkpoint120",
      sMissionName .. "_checkpoint130",
      sMissionName .. "_checkpoint140",
      sMissionName .. "_checkpoint150",
      sMissionName .. "_checkpoint160",
      sMissionName .. "_checkpoint170",
      sMissionName .. "_checkpoint180",
      sMissionName .. "_checkpoint190",
      sMissionName .. "_checkpoint200",
      sMissionName .. "_checkpoint210",
      sMissionName .. "_checkpoint220",
      sMissionName .. "_checkpoint230",
      sMissionName .. "_checkpoint520",
      sMissionName .. "_checkpoint530",
      sMissionName .. "_checkpoint540",
      sMissionName .. "_checkpoint550",
      sMissionName .. "_checkpoint560",
      sMissionName .. "_checkpoint570",
      sMissionName .. "_checkpoint580",
      sMissionName .. "_checkpoint600",
      sMissionName .. "_checkpoint610",
      sMissionName .. "_checkpoint620",
      sMissionName .. "_checkpoint630",
      sMissionName .. "_checkpoint640",
      sMissionName .. "_checkpoint650",
      sMissionName .. "_checkpoint660",
      sMissionName .. "_checkpoint670",
      sMissionName .. "_checkpoint680",
      sMissionName .. "_checkpoint690",
      sMissionName .. "_checkpoint700",
      sMissionName .. "_checkpoint710"
    },
    fOnComplete = function()
      MrxLayerManager.Remove(vLayersStage)
      local uWinner = self.oRaceObjective._uWinner
      if uWinner then
        MrxAchievements.AchievementAddCount("ACHIEVEMENT_HIGHWAY_TO_HELL", 1, uWinner, true)
      end
      local isMultiplayer = Player.GetCurrentPlayers()
      if isMultiplayer == 2 then
        local uWinner = self.oRaceObjective._uWinner
        MrxAchievements.NetGrantAchievement("ACHIEVEMENT_WHEELS_OF_STEEL", uWinner)
      end
      self:Complete()
    end,
    fOnCancel = function()
      self:Cancel()
    end,
    fVehiclesDestroyedCallback = function()
      if 1 < #tVeh then
        self:_SetCancelMessage("[OilCon005.Terms.Cancel01]")
      else
        self:_SetCancelMessage("[OilCon005.Terms.Cancel02]")
      end
    end,
    vVoSeqOnAdd = {
      "Fiona-In-Mission-MinorContract-Oil05-01"
    }
  })
  eCueMusic = self:_CreateEvent(Event.ObjectInSeat, {
    Player.GetAnyCharacter(),
    oCar01,
    "d",
    "e"
  }, MrxMusic.PlaySpecialMusic, {
    "mu_mission_oilcon005_01"
  })
end

function GetSportsCars(self)
  if Net.DoneReloadingLayers then
    Net.DoneReloadingLayers()
  end
  local uSpawn01 = Pg.GetGuidByName("OilCon005_SpawnSportscar01")
  local uSpawn02 = Pg.GetGuidByName("OilCon005_SpawnSportscar02")
  local x1, y1, z1 = Object.GetPosition(uSpawn01)
  local x2, y2, z2 = Object.GetPosition(uSpawn02)
  local nYaw1 = Object.GetYaw(uSpawn01)
  local nYaw2 = Object.GetYaw(uSpawn02)
  local nPlayers = Player.GetCurrentPlayers()
  if nPlayers == 2 then
    oCar01 = Pg.Spawn("Veyron", x1, y1, z1, nYaw1, true, true)
    oCar02 = Pg.Spawn("Veyron", x2, y2, z2, nYaw2, true, true)
    tCars = {oCar01, oCar02}
  else
    oCar01 = Pg.Spawn("Veyron", x1, y1, z1, nYaw1, true, true)
    tCars = {oCar01}
  end
  eAssetsReady = Event.Create(Event.ObjectHibernation, {oCar01, "awake"}, AssetsLoaded, {self})
end

function Cleanup(self)
  Debug.Printf("\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183 OilCon005 is cleaning up now!")
  MrxMusic.StopSpecialMusic()
  MrxLayerManager.MarkForRemoval(vLayersStage)
  MrxLayerManager.MarkForRemoval(vLayersBonus)
  for i, uVeh in ipairs(tCars) do
    Debug.Printf("\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183\176\183 Setting up removal events for " .. tostring(uVeh))
    eRemoveSportsCar = Event.Create(Event.ObjectHibernation, {uVeh, "hibernated"}, Object.Remove, {uVeh})
  end
  MrxTaskContract.Cleanup(self)
end
