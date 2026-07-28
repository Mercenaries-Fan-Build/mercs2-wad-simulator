inherit("MecJob")

function Activated(self)
  self.sVehImg = "global_polaroid_cortez"
  self.sVehLabel = "m35"
  self.sObjText = "[MecJob002.Objectives.001]"
  self.iMinHealth = 30
  self.sIntro = {
    "Fiona-In-Mission-Job-Mec02-01"
  }
  self.sWrongVeh = "Eva-In-Mission-Contract-Mech01-07"
  self.sRightVeh = "Fiona-In-Mission-Job-Mec02-02"
  self.sPropVehTemplate = "Monster Truck phase1"
  MecJob.Activated(self)
end
