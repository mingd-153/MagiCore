import { Component } from '@angular/core';
import { RouterOutlet } from '@angular/router';

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [RouterOutlet],
  template: `
    <h1 [innerText]="projectName"></h1>
    <p>Scaffolded with MegaGate · Angular</p>
    <router-outlet></router-outlet>
  `,
})
export class AppComponent {
  projectName = '{{project_name}}';
}
