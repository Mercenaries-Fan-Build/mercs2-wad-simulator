inherit("MecJob")

function LoadAssets(self, tSaveData)
  local tLayersToAdd = {
    "vz_state_gua_upperclass_pristine",
    "Vz_State_MecJob",
    "Vz_State_MecJob003"
  }
  MrxLayerManager.Add(tLayersToAdd, self.AssetsLoaded, {self})
end

function Activated(self)
  self.sVehImg = "global_polaroid_calderone"
  self.sVehLabel = "amx30"
  self.sObjText = "[MecJob003.Objectives.001]"
  self.iMinHealth = 30
  self.sIntro = {
    "Fiona-In-Mission-Job-Mec03-01"
  }
  self.sWrongVeh = "Eva-In-Mission-Contract-Mech01-08"
  self.sRightVeh = "Fiona-In-Mission-Job-Mec03-03"
  self.sPropVehTemplate = "Monster Truck phase2"
  MecJob.Activated(self)
end

function Cleanup(self)
  MrxLayerManager.MarkForRemoval("Vz_State_MecJob003")
  MecJob.Cleanup(self)
end
